use async_trait::async_trait;
use thiserror::Error;

/// API key record. Only the secret's hash is stored; the plaintext appears exactly once,
/// in the create response.
#[derive(Debug, Clone)]
pub struct ApiKeyRecord {
    pub id: String,
    pub name: String,
    pub secret_hash: String,
    /// Raw permission strings (`RESOURCE:A+B`), normalized via PermissionSet before storage.
    pub permissions: Vec<String>,
    pub created_at_ms: i64,
    pub revoked: bool,
    /// Owner user id; legacy admin-created keys have no owner (empty string).
    pub user_id: String,
    /// Expiry instant (epoch ms); None = never expires.
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApiKeyRepoError {
    #[error("apikey name already exists")]
    NameExists,
    #[error("apikey backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    async fn insert(
        &self,
        id: &str,
        name: &str,
        secret_hash: &str,
        permissions: &[String],
        user_id: &str,
        expires_at_ms: Option<i64>,
    ) -> Result<ApiKeyRecord, ApiKeyRepoError>;

    async fn list(&self) -> Result<Vec<ApiKeyRecord>, ApiKeyRepoError>;

    /// Personal view: only keys owned by user_id.
    async fn list_by_user(&self, user_id: &str) -> Result<Vec<ApiKeyRecord>, ApiKeyRepoError>;

    /// Admin path: fetch by id without filtering revoked/expired (used for ownership checks).
    async fn find(&self, id: &str) -> Result<Option<ApiKeyRecord>, ApiKeyRepoError>;

    /// Auth path: only returns records that are neither revoked nor expired.
    async fn find_active(&self, id: &str) -> Result<Option<ApiKeyRecord>, ApiKeyRepoError>;

    /// Revoke (set revoked=true). Returns `false` if the id is missing or already revoked.
    async fn revoke(&self, id: &str) -> Result<bool, ApiKeyRepoError>;

    /// Enable/disable: enabled=false equals revoke, true restores; idempotent. Returns
    /// `false` if the id does not exist.
    async fn set_enabled(&self, id: &str, enabled: bool) -> Result<bool, ApiKeyRepoError>;
}
