use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{ExternalIdentity, OidcError};
use crate::ports::{ExternalIdentityProvider, ExternalUserRepository, LinkedUser};

/// `exchange` fails for code "bad", otherwise returns the preset identity.
#[derive(Clone)]
pub struct FakeIdentityProvider {
    key: String,
    identity: ExternalIdentity,
}

impl FakeIdentityProvider {
    pub fn new(key: &str, identity: ExternalIdentity) -> Self {
        Self { key: key.to_string(), identity }
    }
}

#[async_trait]
impl ExternalIdentityProvider for FakeIdentityProvider {
    fn key(&self) -> &str {
        &self.key
    }

    fn authorize_url(&self, state: &str) -> String {
        format!("https://fake.example/{}/authorize?state={state}", self.key)
    }

    async fn exchange(&self, code: &str) -> Result<ExternalIdentity, OidcError> {
        if code == "bad" {
            return Err(OidcError::Exchange("invalid code".into()));
        }
        Ok(self.identity.clone())
    }
}

#[derive(Clone, Default)]
pub struct InMemoryExternalUserRepository {
    links: Arc<Mutex<HashMap<(String, String), LinkedUser>>>,
    provisioned: Arc<Mutex<usize>>,
}

impl InMemoryExternalUserRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn provisioned_count(&self) -> usize {
        *self.provisioned.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[async_trait]
impl ExternalUserRepository for InMemoryExternalUserRepository {
    async fn find_or_provision(
        &self,
        identity: &ExternalIdentity,
        default_permissions: &[String],
    ) -> Result<LinkedUser, OidcError> {
        let key = (identity.provider.clone(), identity.open_id.clone());
        let mut links = self.links.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(u) = links.get(&key) {
            return Ok(u.clone());
        }
        *self.provisioned.lock().unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
        let linked = LinkedUser {
            user_id: format!("ext-{}-{}", identity.provider, identity.open_id),
            permissions: default_permissions.to_vec(),
        };
        links.insert(key, linked.clone());
        Ok(linked)
    }
}
