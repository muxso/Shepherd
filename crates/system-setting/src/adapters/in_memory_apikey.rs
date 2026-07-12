use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use crate::ports::{ApiKeyRecord, ApiKeyRepoError, ApiKeyRepository};

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// In-memory implementation for tests/local; BTreeMap keeps list order stable.
#[derive(Clone, Default)]
pub struct InMemoryApiKeyRepository {
    state: Arc<Mutex<BTreeMap<String, ApiKeyRecord>>>,
}

impl InMemoryApiKeyRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ApiKeyRepository for InMemoryApiKeyRepository {
    async fn insert(
        &self,
        id: &str,
        name: &str,
        secret_hash: &str,
        permissions: &[String],
        user_id: &str,
        expires_at_ms: Option<i64>,
    ) -> Result<ApiKeyRecord, ApiKeyRepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if st.values().any(|r| r.name == name) {
            return Err(ApiKeyRepoError::NameExists);
        }
        let rec = ApiKeyRecord {
            id: id.to_string(),
            name: name.to_string(),
            secret_hash: secret_hash.to_string(),
            permissions: permissions.to_vec(),
            created_at_ms: now_ms(),
            revoked: false,
            user_id: user_id.to_string(),
            expires_at_ms,
        };
        st.insert(id.to_string(), rec.clone());
        Ok(rec)
    }

    async fn list(&self) -> Result<Vec<ApiKeyRecord>, ApiKeyRepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st.values().cloned().collect())
    }

    async fn list_by_user(&self, user_id: &str) -> Result<Vec<ApiKeyRecord>, ApiKeyRepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st.values().filter(|r| r.user_id == user_id).cloned().collect())
    }

    async fn find(&self, id: &str) -> Result<Option<ApiKeyRecord>, ApiKeyRepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st.get(id).cloned())
    }

    async fn find_active(&self, id: &str) -> Result<Option<ApiKeyRecord>, ApiKeyRepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st
            .get(id)
            .filter(|r| !r.revoked && r.expires_at_ms.is_none_or(|exp| exp > now_ms()))
            .cloned())
    }

    async fn revoke(&self, id: &str) -> Result<bool, ApiKeyRepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match st.get_mut(id) {
            Some(r) if !r.revoked => {
                r.revoked = true;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn set_enabled(&self, id: &str, enabled: bool) -> Result<bool, ApiKeyRepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match st.get_mut(id) {
            Some(r) => {
                r.revoked = !enabled;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}
