use crate::pg::PgProfileStore;
use async_trait::async_trait;
use plugin_runtime::{
    ExecutionAuthorizer, ExecutionScope, PluginError, SecretRef, SecretResolver, SecretValue,
};
use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::{collections::BTreeMap, sync::Arc};

pub struct Keyring {
    active: String,
    keys: BTreeMap<String, LessSafeKey>,
}
struct Sealed {
    key_id: String,
    nonce: [u8; 12],
    ciphertext: Vec<u8>,
}
impl Keyring {
    /// Callers load keys from deployment secrets. Keeping previous ids permits
    /// deliberate key rotation without silently losing old credentials.
    pub fn new(active: String, keys: BTreeMap<String, [u8; 32]>) -> Result<Self, PluginError> {
        if active.is_empty()
            || !keys.contains_key(&active)
            || keys.keys().any(|key| key.trim().is_empty() || key.len() > 128)
        {
            return Err(PluginError::InvalidConfig("invalid credential encryption keyring".into()));
        }
        let keys = keys
            .into_iter()
            .map(|(id, bytes)| {
                UnboundKey::new(&aead::AES_256_GCM, &bytes)
                    .map(|key| (id, LessSafeKey::new(key)))
                    .map_err(|_| {
                        PluginError::InvalidConfig("invalid credential encryption key".into())
                    })
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { active, keys })
    }
    fn aad(
        project: &str,
        environment: Option<&str>,
        id: &str,
        key: &str,
    ) -> Result<Vec<u8>, PluginError> {
        serde_json::to_vec(&("shepherd.environment.credentials.v1", project, environment, id, key))
            .map_err(|_| PluginError::PermissionDenied)
    }
    fn seal(
        &self,
        project: &str,
        environment: Option<&str>,
        id: &str,
        value: &SecretValue,
    ) -> Result<Sealed, PluginError> {
        if value.expose().len() > 65_536 {
            return Err(PluginError::InvalidConfig("credential exceeds size limit".into()));
        }
        let mut nonce = [0u8; 12];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| PluginError::Unavailable("credential encryption".into()))?;
        let mut ciphertext = value.expose().as_bytes().to_vec();
        let aad = Self::aad(project, environment, id, &self.active)?;
        self.keys
            .get(&self.active)
            .ok_or(PluginError::PermissionDenied)?
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad),
                &mut ciphertext,
            )
            .map_err(|_| PluginError::Unavailable("credential encryption".into()))?;
        Ok(Sealed { key_id: self.active.clone(), nonce, ciphertext })
    }
    fn open(
        &self,
        project: &str,
        environment: Option<&str>,
        id: &str,
        sealed: Sealed,
    ) -> Result<SecretValue, PluginError> {
        let aad = Self::aad(project, environment, id, &sealed.key_id)?;
        let mut buffer = sealed.ciphertext;
        let plain = self
            .keys
            .get(&sealed.key_id)
            .ok_or(PluginError::PermissionDenied)?
            .open_in_place(Nonce::assume_unique_for_key(sealed.nonce), Aad::from(aad), &mut buffer)
            .map_err(|_| PluginError::PermissionDenied)?;
        let value =
            std::str::from_utf8(plain).map_err(|_| PluginError::PermissionDenied)?.to_string();
        Ok(SecretValue::new(value))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialMetadata {
    pub id: String,
    pub label: String,
    pub environment_id: Option<String>,
}

#[derive(Clone)]
pub struct CredentialVault {
    store: PgProfileStore,
    keys: Arc<Keyring>,
    authorization: Arc<dyn ExecutionAuthorizer>,
}
impl CredentialVault {
    pub fn new(
        store: PgProfileStore,
        keys: Arc<Keyring>,
        authorization: Arc<dyn ExecutionAuthorizer>,
    ) -> Self {
        Self { store, keys, authorization }
    }
    async fn authorize(&self, scope: &ExecutionScope, operation: &str) -> Result<(), PluginError> {
        scope.validate()?;
        self.authorization.authorize(scope, "credentials", operation).await
    }
    pub async fn create(
        &self,
        scope: &ExecutionScope,
        label: &str,
        environment_only: bool,
        value: SecretValue,
    ) -> Result<SecretRef, PluginError> {
        self.authorize(scope, "create").await?;
        if label.trim().is_empty() || label.len() > 200 {
            return Err(PluginError::InvalidConfig("invalid credential label".into()));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let environment = environment_only.then_some(scope.environment_id.as_str());
        let sealed = self.keys.seal(&scope.project_id, environment, &id, &value)?;
        sqlx::query("INSERT INTO ep_secret(id,project_id,environment_id,label,key_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(&id).bind(&scope.project_id).bind(environment).bind(label.trim()).bind(sealed.key_id).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext)
            .execute(&self.store.pool).await.map_err(storage)?;
        Ok(SecretRef(id))
    }
    pub async fn list(
        &self,
        scope: &ExecutionScope,
    ) -> Result<Vec<CredentialMetadata>, PluginError> {
        self.authorize(scope, "list").await?;
        sqlx::query("SELECT id,label,environment_id FROM ep_secret WHERE project_id=$1 AND (environment_id IS NULL OR environment_id=$2) ORDER BY label,id")
            .bind(&scope.project_id).bind(&scope.environment_id).fetch_all(&self.store.pool).await.map_err(storage)?.into_iter().map(|row| {
                Ok(CredentialMetadata { id:row.try_get("id").map_err(storage)?,label:row.try_get("label").map_err(storage)?,environment_id:row.try_get("environment_id").map_err(storage)? })
            }).collect()
    }
    pub async fn delete(
        &self,
        scope: &ExecutionScope,
        reference: &SecretRef,
    ) -> Result<(), PluginError> {
        self.authorize(scope, "delete").await?;
        let deleted=sqlx::query("DELETE FROM ep_secret WHERE project_id=$1 AND id=$2 AND (environment_id IS NULL OR environment_id=$3)")
            .bind(&scope.project_id).bind(&reference.0).bind(&scope.environment_id).execute(&self.store.pool).await.map_err(storage)?.rows_affected();
        if deleted == 0 {
            return Err(PluginError::PermissionDenied);
        }
        Ok(())
    }
}
fn storage(_: sqlx::Error) -> PluginError {
    PluginError::Unavailable("credential storage".into())
}

#[async_trait]
impl SecretResolver for CredentialVault {
    async fn resolve(
        &self,
        scope: &ExecutionScope,
        reference: &SecretRef,
    ) -> Result<SecretValue, PluginError> {
        self.authorize(scope, "resolve").await?;
        let row=sqlx::query("SELECT environment_id,key_id,nonce,ciphertext FROM ep_secret WHERE project_id=$1 AND id=$2 AND (environment_id IS NULL OR environment_id=$3)")
            .bind(&scope.project_id).bind(&reference.0).bind(&scope.environment_id).fetch_optional(&self.store.pool).await.map_err(storage)?.ok_or(PluginError::PermissionDenied)?;
        let environment: Option<String> = row.try_get("environment_id").map_err(storage)?;
        let nonce: Vec<u8> = row.try_get("nonce").map_err(storage)?;
        let sealed = Sealed {
            key_id: row.try_get("key_id").map_err(storage)?,
            nonce: nonce.try_into().map_err(|_| PluginError::PermissionDenied)?,
            ciphertext: row.try_get("ciphertext").map_err(storage)?,
        };
        self.keys.open(&scope.project_id, environment.as_deref(), &reference.0, sealed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encryption_binds_scope_identity_and_key_with_fresh_nonces() {
        let keys = Keyring::new("v1".into(), [("v1".into(), [7; 32])].into()).expect("keys");
        let value = SecretValue::new("credential-with-unicode-密码".into());
        let one = keys.seal("project", Some("env"), "id", &value).expect("seal");
        let two = keys.seal("project", Some("env"), "id", &value).expect("seal");
        assert_ne!(one.nonce, two.nonce);
        assert!(!one
            .ciphertext
            .windows(value.expose().len())
            .any(|w| w == value.expose().as_bytes()));
        assert_eq!(
            keys.open("project", Some("env"), "id", one).expect("open").expose(),
            value.expose()
        );
        for (project, env, id) in [
            ("foreign", Some("env"), "id"),
            ("project", None, "id"),
            ("project", Some("env"), "foreign-id"),
        ] {
            assert!(keys
                .open(
                    project,
                    env,
                    id,
                    keys.seal("project", Some("env"), "id", &value).expect("seal")
                )
                .is_err());
        }
        let mut tampered = keys.seal("project", None, "id", &value).expect("seal");
        tampered.ciphertext[0] ^= 1;
        assert!(keys.open("project", None, "id", tampered).is_err());
        let old = keys.seal("project", None, "id", &value).expect("seal");
        let rotated =
            Keyring::new("v2".into(), [("v1".into(), [7; 32]), ("v2".into(), [8; 32])].into())
                .expect("rotated keys");
        assert_eq!(
            rotated.open("project", None, "id", old).expect("old key remains readable").expose(),
            value.expose()
        );
    }
}
