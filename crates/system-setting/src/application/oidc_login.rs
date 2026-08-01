use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use kernel::permission::PermissionSet;

use crate::domain::{OidcError, OidcProvider};
use crate::ports::{
    ExternalIdentityProvider, ExternalUserRepository, OidcProviderRepository, OidcRepoError,
    SessionStore,
};

/// A provider registered in the live runtime registry: the exchange strategy
/// plus the permission set granted to users provisioned through it on first
/// login.
#[derive(Clone)]
struct RegisteredProvider {
    strategy: Arc<dyn ExternalIdentityProvider>,
    default_permissions: Vec<String>,
}

#[derive(Clone)]
pub struct OidcLoginUseCase {
    // Live, mutable registry. Behind an RwLock so the admin API can swap the
    // strategy set at runtime (after a DB mutation) without rebuilding the
    // use case. Reads clone the entry out and drop the guard before any `.await`
    // so the lock is never held across an await point.
    providers: Arc<RwLock<HashMap<String, RegisteredProvider>>>,
    users: Arc<dyn ExternalUserRepository>,
    sessions: Arc<dyn SessionStore>,
    ttl_secs: i64,
}

impl OidcLoginUseCase {
    pub fn new(users: Arc<dyn ExternalUserRepository>, sessions: Arc<dyn SessionStore>) -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            users,
            sessions,
            ttl_secs: 8 * 3600,
        }
    }

    /// Builder-style registration (used e.g. for env-seeded providers). With
    /// DB-backed providers prefer [`OidcLoginUseCase::reload`]. `default_permissions`
    /// is granted to users provisioned through this provider on first login.
    pub fn register(
        self,
        provider: Arc<dyn ExternalIdentityProvider>,
        default_permissions: Vec<String>,
    ) -> Self {
        self.providers.write().expect("oidc registry poisoned").insert(
            provider.key().to_string(),
            RegisteredProvider { strategy: provider, default_permissions },
        );
        self
    }

    pub fn unregister(&self, key: &str) {
        self.providers.write().expect("oidc registry poisoned").remove(key);
    }

    pub fn with_ttl_secs(mut self, secs: i64) -> Self {
        self.ttl_secs = secs;
        self
    }

    pub fn provider_keys(&self) -> Vec<String> {
        let guard = self.providers.read().expect("oidc registry poisoned");
        let mut v: Vec<String> = guard.keys().cloned().collect();
        v.sort();
        v
    }

    pub fn authorize_url(&self, provider: &str, state: &str) -> Result<String, OidcError> {
        let p = self
            .provider(provider)?
            .ok_or_else(|| OidcError::UnknownProvider(provider.to_string()))?;
        Ok(p.strategy.authorize_url(state))
    }

    pub async fn complete(&self, provider: &str, code: &str) -> Result<String, OidcError> {
        let p = self
            .provider(provider)?
            .ok_or_else(|| OidcError::UnknownProvider(provider.to_string()))?;
        let identity = p.strategy.exchange(code).await?;
        let linked = self.users.find_or_provision(&identity, &p.default_permissions).await?;
        let permissions = PermissionSet::from_raw(&linked.permissions)
            .map_err(|_| OidcError::Backend("invalid permission config".into()))?;
        self.sessions
            .create(&linked.user_id, permissions, self.ttl_secs)
            .await
            .map_err(|e| OidcError::Backend(e.to_string()))
    }

    /// Reads a provider out of the registry without holding the read lock
    /// across an await point (the caller may `.await` on the returned entry).
    fn provider(&self, key: &str) -> Result<Option<RegisteredProvider>, OidcError> {
        let guard = self.providers.read().expect("oidc registry poisoned");
        Ok(guard.get(key).cloned())
    }

    /// Rebuilds the live registry from the enabled rows of `repo`. The
    /// `build` closure maps a stored [`OidcProvider`] to its strategy object;
    /// it is injected by the composition root so this always-compiled layer
    /// never depends on the (feature-gated) adapter layer.
    pub async fn reload<F>(
        &self,
        repo: &dyn OidcProviderRepository,
        build: F,
    ) -> Result<(), OidcRepoError>
    where
        F: Fn(&OidcProvider) -> Option<Arc<dyn ExternalIdentityProvider>>,
    {
        let enabled = repo.list_enabled().await?;
        let mut map: HashMap<String, RegisteredProvider> = HashMap::new();
        for cfg in &enabled {
            if let Some(strategy) = build(cfg) {
                map.insert(
                    strategy.key().to_string(),
                    RegisteredProvider {
                        strategy,
                        default_permissions: cfg.default_permissions.clone(),
                    },
                );
            }
        }
        *self.providers.write().expect("oidc registry poisoned") = map;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;

    use crate::adapters::{
        FakeIdentityProvider, InMemoryExternalUserRepository, InMemorySessionStore,
    };
    use crate::domain::{ExternalIdentity, OidcProvider};
    use crate::ports::OidcProviderRepository;

    fn uc() -> (OidcLoginUseCase, Arc<InMemorySessionStore>, Arc<InMemoryExternalUserRepository>) {
        let sessions = Arc::new(InMemorySessionStore::new());
        let users = Arc::new(InMemoryExternalUserRepository::new());
        let feishu = FakeIdentityProvider::new(
            "feishu",
            ExternalIdentity {
                provider: "feishu".into(),
                open_id: "ou_alice".into(),
                display_name: "Alice".into(),
            },
        );
        let uc = OidcLoginUseCase::new(users.clone(), sessions.clone())
            .register(Arc::new(feishu), vec!["PROJECT:READ".into()]);
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
        // FakeIdentityProvider contract: code "bad" → exchange failure.
        assert!(matches!(uc.complete("feishu", "bad").await, Err(OidcError::Exchange(_))));
    }

    #[tokio::test]
    async fn relogin_maps_to_same_local_user() {
        let (uc, _sessions, users) = uc();
        uc.complete("feishu", "c1").await.expect("first");
        uc.complete("feishu", "c2").await.expect("second");
        assert_eq!(users.provisioned_count(), 1);
    }

    /// Tiny in-memory OidcProviderRepository so the reload path can be tested
    /// without a database.
    struct InMemProviders {
        inner: Mutex<Vec<OidcProvider>>,
    }
    impl InMemProviders {
        fn new(items: Vec<OidcProvider>) -> Self {
            Self { inner: Mutex::new(items) }
        }
    }
    #[async_trait]
    impl OidcProviderRepository for InMemProviders {
        async fn list(&self) -> Result<Vec<OidcProvider>, OidcRepoError> {
            Ok(self.inner.lock().unwrap().clone())
        }
        async fn list_enabled(&self) -> Result<Vec<OidcProvider>, OidcRepoError> {
            Ok(self.inner.lock().unwrap().iter().filter(|p| p.enabled).cloned().collect())
        }
        async fn get(&self, key: &str) -> Result<Option<OidcProvider>, OidcRepoError> {
            Ok(self.inner.lock().unwrap().iter().find(|p| p.provider_key == key).cloned())
        }
        async fn upsert(&self, p: &OidcProvider) -> Result<(), OidcRepoError> {
            let mut g = self.inner.lock().unwrap();
            if let Some(existing) = g.iter_mut().find(|x| x.provider_key == p.provider_key) {
                *existing = p.clone();
            } else {
                g.push(p.clone());
            }
            Ok(())
        }
        async fn delete(&self, key: &str) -> Result<(), OidcRepoError> {
            self.inner.lock().unwrap().retain(|p| p.provider_key != key);
            Ok(())
        }
    }

    fn fake_factory(p: &OidcProvider) -> Option<Arc<dyn ExternalIdentityProvider>> {
        Some(Arc::new(FakeIdentityProvider::new(
            p.provider_key.as_str(),
            ExternalIdentity {
                provider: p.provider_key.clone(),
                open_id: "ou".into(),
                display_name: "N".into(),
            },
        )))
    }

    #[tokio::test]
    async fn reload_rebuilds_registry_from_repo() {
        let sessions = Arc::new(InMemorySessionStore::new());
        let users = Arc::new(InMemoryExternalUserRepository::new());
        let uc = OidcLoginUseCase::new(users.clone(), sessions.clone());

        // Empty repo => empty registry (even after an earlier registration).
        let uc = uc.register(
            Arc::new(FakeIdentityProvider::new(
                "feishu",
                ExternalIdentity {
                    provider: "feishu".into(),
                    open_id: "ou".into(),
                    display_name: "N".into(),
                },
            )),
            vec!["PROJECT:READ".into()],
        );
        uc.reload(&InMemProviders::new(vec![]), fake_factory).await.unwrap();
        assert!(uc.provider_keys().is_empty());

        // A single enabled row becomes a registered provider keyed by its key.
        let repo = InMemProviders::new(vec![OidcProvider {
            provider_key: "feishu".into(),
            app_id: "x".into(),
            app_secret: "y".into(),
            redirect: String::new(),
            default_permissions: vec!["PROJECT:READ".into()],
            enabled: true,
            base_url: None,
        }]);
        uc.reload(&repo, fake_factory).await.unwrap();
        assert_eq!(uc.provider_keys(), vec!["feishu".to_string()]);

        // Disabled rows are excluded by list_enabled.
        let repo = InMemProviders::new(vec![
            OidcProvider {
                provider_key: "on".into(),
                app_id: "a".into(),
                app_secret: "b".into(),
                redirect: String::new(),
                default_permissions: vec![],
                enabled: true,
                base_url: None,
            },
            OidcProvider {
                provider_key: "off".into(),
                app_id: "c".into(),
                app_secret: "d".into(),
                redirect: String::new(),
                default_permissions: vec![],
                enabled: false,
                base_url: None,
            },
        ]);
        uc.reload(&repo, fake_factory).await.unwrap();
        assert_eq!(uc.provider_keys(), vec!["on".to_string()]);
    }
}
