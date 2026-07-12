use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use kernel::permission::PermissionSet;

use crate::ports::{ApiKeyRepository, AuthRepoError, PasswordHasher, SessionStore};
use webauth::Session;

const TOKEN_PREFIX: &str = "sak_";
/// Cache TTL, and thus the maximum delay before a revocation takes effect.
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(60);

struct CachedKey {
    cached_at: Instant,
    user_id: String,
    permissions: PermissionSet,
}

/// Composite session store: tokens with the `sak_` prefix go through API key verification,
/// everything else is delegated to the inner store (e.g. PgSessionStore). Zero changes at
/// the routing layer — it still sees a single `SessionStore`.
///
/// Argon2 verification is too slow to run per request: a successfully verified key is
/// cached in-process for 60s; after expiry we re-check the source (find_active + re-verify
/// hash), so a revoked key survives at most one cache period.
pub struct ApiKeySessionStore {
    inner: Arc<dyn SessionStore>,
    keys: Arc<dyn ApiKeyRepository>,
    hasher: Arc<dyn PasswordHasher>,
    cache_ttl: Duration,
    cache: Mutex<HashMap<String, CachedKey>>,
}

impl ApiKeySessionStore {
    pub fn new(
        inner: Arc<dyn SessionStore>,
        keys: Arc<dyn ApiKeyRepository>,
        hasher: Arc<dyn PasswordHasher>,
    ) -> Self {
        Self {
            inner,
            keys,
            hasher,
            cache_ttl: DEFAULT_CACHE_TTL,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Test hook: shorten the cache period (`Duration::ZERO` = always re-check the source).
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    fn cache_get(&self, token: &str) -> Option<Session> {
        let cache = self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let hit = cache.get(token)?;
        if hit.cached_at.elapsed() >= self.cache_ttl {
            return None;
        }
        Some(session_for(token, &hit.user_id, hit.permissions.clone()))
    }

    fn cache_put(&self, token: &str, user_id: &str, permissions: PermissionSet) {
        self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(
            token.to_string(),
            CachedKey { cached_at: Instant::now(), user_id: user_id.to_string(), permissions },
        );
    }

    fn cache_evict(&self, token: &str) {
        self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(token);
    }
}

fn session_for(token: &str, user_id: &str, permissions: PermissionSet) -> Session {
    // API keys have no session-expiry semantics (revocation uses the revoked flag), so
    // expires_at_ms is set to the maximum.
    Session {
        token: token.to_string(),
        user_id: user_id.to_string(),
        permissions,
        expires_at_ms: i64::MAX,
    }
}

fn is_lower_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Parse `sak_<id>.<secret>`; malformed input returns None (no fallthrough to the inner
/// store, to avoid querying the DB with a secret as if it were a plain token).
fn parse_token(token: &str) -> Option<(&str, &str)> {
    let rest = token.strip_prefix(TOKEN_PREFIX)?;
    let (id, secret) = rest.split_once('.')?;
    (is_lower_hex(id) && is_lower_hex(secret)).then_some((id, secret))
}

#[async_trait]
impl SessionStore for ApiKeySessionStore {
    async fn create(
        &self,
        user_id: &str,
        permissions: PermissionSet,
        ttl_secs: i64,
    ) -> Result<String, AuthRepoError> {
        self.inner.create(user_id, permissions, ttl_secs).await
    }

    async fn get(&self, token: &str) -> Result<Option<Session>, AuthRepoError> {
        if !token.starts_with(TOKEN_PREFIX) {
            return self.inner.get(token).await;
        }
        if let Some(session) = self.cache_get(token) {
            return Ok(Some(session));
        }
        let Some((id, secret)) = parse_token(token) else {
            return Ok(None);
        };
        let rec =
            self.keys.find_active(id).await.map_err(|e| AuthRepoError::Backend(e.to_string()))?;
        let Some(rec) = rec else {
            self.cache_evict(token);
            return Ok(None);
        };
        if !self.hasher.verify(secret, &rec.secret_hash) {
            self.cache_evict(token);
            return Ok(None);
        }
        let permissions = PermissionSet::from_raw(&rec.permissions)
            .map_err(|_| AuthRepoError::Backend("corrupt apikey permissions".into()))?;
        let user_id = format!("apikey:{}", rec.name);
        self.cache_put(token, &user_id, permissions.clone());
        Ok(Some(session_for(token, &user_id, permissions)))
    }

    async fn revoke(&self, token: &str) -> Result<(), AuthRepoError> {
        // API keys can't be revoked by token (logout semantics); revocation goes through
        // DELETE /system/apikey/{id}.
        if token.starts_with(TOKEN_PREFIX) {
            return Ok(());
        }
        self.inner.revoke(token).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{InMemoryApiKeyRepository, InMemorySessionStore, PlainPasswordHasher};

    const ID: &str = "0123456789abcdef";
    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    fn store(ttl: Duration) -> (ApiKeySessionStore, Arc<InMemoryApiKeyRepository>) {
        let keys = Arc::new(InMemoryApiKeyRepository::new());
        let s = ApiKeySessionStore::new(
            Arc::new(InMemorySessionStore::new()),
            keys.clone(),
            Arc::new(PlainPasswordHasher),
        )
        .with_cache_ttl(ttl);
        (s, keys)
    }

    async fn seed(keys: &InMemoryApiKeyRepository) {
        // PlainPasswordHasher: hash == plaintext.
        keys.insert(ID, "runner-1", SECRET, &["DELIVERY:READ+UPDATE".to_string()], "u-owner", None)
            .await
            .expect("seed");
    }

    fn token() -> String {
        format!("sak_{ID}.{SECRET}")
    }

    #[tokio::test]
    async fn sak_token_authenticates_with_key_permissions() {
        let (store, keys) = store(DEFAULT_CACHE_TTL);
        seed(&keys).await;
        let s = store.get(&token()).await.expect("ok").expect("some");
        assert_eq!(s.user_id, "apikey:runner-1");
        assert!(s.permissions.allows("DELIVERY", "UPDATE"));
        assert!(!s.permissions.allows("SYSTEM_USER", "READ"));
    }

    #[tokio::test]
    async fn wrong_secret_or_unknown_id_rejected() {
        let (store, keys) = store(DEFAULT_CACHE_TTL);
        seed(&keys).await;
        let bad = format!("sak_{ID}.{}", "f".repeat(32));
        assert!(store.get(&bad).await.expect("ok").is_none());
        let ghost = format!("sak_{}.{SECRET}", "f".repeat(16));
        assert!(store.get(&ghost).await.expect("ok").is_none());
    }

    #[tokio::test]
    async fn malformed_sak_tokens_rejected_without_hitting_inner() {
        let (store, _keys) = store(DEFAULT_CACHE_TTL);
        for t in ["sak_", "sak_nodot", "sak_.", &format!("sak_{ID}."), "sak_UPPER.HEX"] {
            assert!(store.get(t).await.expect("ok").is_none(), "{t}");
        }
    }

    #[tokio::test]
    async fn expired_key_rejected() {
        let (store, keys) = store(Duration::ZERO);
        keys.insert(ID, "runner-1", SECRET, &[], "u-owner", Some(1)).await.expect("seed expired");
        assert!(store.get(&token()).await.expect("ok").is_none());
    }

    #[tokio::test]
    async fn revoked_key_rejected_after_cache_expiry() {
        // TTL=0: every get re-checks the source, so revocation takes effect immediately.
        let (store, keys) = store(Duration::ZERO);
        seed(&keys).await;
        assert!(store.get(&token()).await.expect("ok").is_some());
        assert!(keys.revoke(ID).await.expect("ok"));
        assert!(store.get(&token()).await.expect("ok").is_none());
    }

    #[tokio::test]
    async fn cache_serves_within_ttl_even_after_revoke() {
        // Documents the cache semantics: within the TTL a revocation is not yet effective
        // (a bounded grace period).
        let (store, keys) = store(DEFAULT_CACHE_TTL);
        seed(&keys).await;
        assert!(store.get(&token()).await.expect("ok").is_some());
        assert!(keys.revoke(ID).await.expect("ok"));
        assert!(store.get(&token()).await.expect("ok").is_some());
    }

    #[tokio::test]
    async fn non_sak_tokens_delegate_to_inner_store() {
        let (store, _keys) = store(DEFAULT_CACHE_TTL);
        let perms = PermissionSet::from_raw(["PROJECT:READ"]).expect("perm");
        let tok = store.create("u-1", perms, 3600).await.expect("create");
        let s = store.get(&tok).await.expect("ok").expect("some");
        assert_eq!(s.user_id, "u-1");
        store.revoke(&tok).await.expect("revoke");
        assert!(store.get(&tok).await.expect("ok").is_none());
    }

    #[tokio::test]
    async fn revoking_sak_token_is_noop() {
        let (store, keys) = store(Duration::ZERO);
        seed(&keys).await;
        store.revoke(&token()).await.expect("noop");
        assert!(store.get(&token()).await.expect("ok").is_some());
    }
}
