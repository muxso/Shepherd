use std::collections::HashMap;
use std::sync::Arc;

use kernel::permission::PermissionSet;

use crate::domain::OidcError;
use crate::ports::{ExternalIdentityProvider, ExternalUserRepository, SessionStore};

#[derive(Clone)]
pub struct OidcLoginUseCase {
    providers: HashMap<String, Arc<dyn ExternalIdentityProvider>>,
    users: Arc<dyn ExternalUserRepository>,
    sessions: Arc<dyn SessionStore>,
    ttl_secs: i64,
}

impl OidcLoginUseCase {
    pub fn new(users: Arc<dyn ExternalUserRepository>, sessions: Arc<dyn SessionStore>) -> Self {
        Self { providers: HashMap::new(), users, sessions, ttl_secs: 8 * 3600 }
    }

    pub fn register(mut self, provider: Arc<dyn ExternalIdentityProvider>) -> Self {
        self.providers.insert(provider.key().to_string(), provider);
        self
    }

    pub fn with_ttl_secs(mut self, secs: i64) -> Self {
        self.ttl_secs = secs;
        self
    }

    pub fn provider_keys(&self) -> Vec<String> {
        let mut v: Vec<String> = self.providers.keys().cloned().collect();
        v.sort();
        v
    }

    pub fn authorize_url(&self, provider: &str, state: &str) -> Result<String, OidcError> {
        self.providers
            .get(provider)
            .map(|p| p.authorize_url(state))
            .ok_or_else(|| OidcError::UnknownProvider(provider.to_string()))
    }

    pub async fn complete(&self, provider: &str, code: &str) -> Result<String, OidcError> {
        let p = self
            .providers
            .get(provider)
            .ok_or_else(|| OidcError::UnknownProvider(provider.to_string()))?;
        let identity = p.exchange(code).await?;
        let linked = self.users.find_or_provision(&identity).await?;
        let permissions = PermissionSet::from_raw(&linked.permissions)
            .map_err(|_| OidcError::Backend("invalid permission config".into()))?;
        self.sessions
            .create(&linked.user_id, permissions, self.ttl_secs)
            .await
            .map_err(|e| OidcError::Backend(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{
        FakeIdentityProvider, InMemoryExternalUserRepository, InMemorySessionStore,
    };
    use crate::domain::ExternalIdentity;

    fn uc() -> (OidcLoginUseCase, Arc<InMemorySessionStore>, Arc<InMemoryExternalUserRepository>) {
        let sessions = Arc::new(InMemorySessionStore::new());
        let users = Arc::new(InMemoryExternalUserRepository::new(["PROJECT:READ"]));
        let feishu = FakeIdentityProvider::new(
            "feishu",
            ExternalIdentity {
                provider: "feishu".into(),
                open_id: "ou_alice".into(),
                display_name: "Alice".into(),
            },
        );
        let uc = OidcLoginUseCase::new(users.clone(), sessions.clone()).register(Arc::new(feishu));
        (uc, sessions, users)
    }

    #[tokio::test]
    async fn authorize_url_for_registered_provider() {
        let (uc, _, _) = uc();
        let url = uc.authorize_url("feishu", "xyz").expect("ok");
        assert!(url.contains("state=xyz"));
    }

    #[tokio::test]
    async fn unknown_provider_errors() {
        let (uc, _, _) = uc();
        assert_eq!(
            uc.authorize_url("github", "s"),
            Err(OidcError::UnknownProvider("github".into()))
        );
        assert_eq!(
            uc.complete("github", "code").await,
            Err(OidcError::UnknownProvider("github".into()))
        );
    }

    #[tokio::test]
    async fn good_code_yields_session_token() {
        let (uc, sessions, _) = uc();
        let token = uc.complete("feishu", "valid-code").await.expect("login");
        let session = sessions.get(&token).await.expect("ok").expect("session");
        assert!(session.permissions.allows("PROJECT", "READ"));
    }

    #[tokio::test]
    async fn exchange_failure_propagates() {
        let (uc, _, _) = uc();
        // FakeIdentityProvider 约定:code "bad" → 交换失败
        assert!(matches!(uc.complete("feishu", "bad").await, Err(OidcError::Exchange(_))));
    }

    #[tokio::test]
    async fn relogin_maps_to_same_local_user() {
        let (uc, _sessions, users) = uc();
        uc.complete("feishu", "c1").await.expect("first");
        uc.complete("feishu", "c2").await.expect("second");
        assert_eq!(users.provisioned_count(), 1);
    }
}
