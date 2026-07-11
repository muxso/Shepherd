use async_trait::async_trait;
use thiserror::Error;

/// API Key 记录。secret 只存哈希;明文仅在创建响应里出现一次。
#[derive(Debug, Clone)]
pub struct ApiKeyRecord {
    pub id: String,
    pub name: String,
    pub secret_hash: String,
    /// 原始权限串(`RESOURCE:A+B`),入库前已按 PermissionSet 规范化。
    pub permissions: Vec<String>,
    pub created_at_ms: i64,
    pub revoked: bool,
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
    ) -> Result<ApiKeyRecord, ApiKeyRepoError>;

    async fn list(&self) -> Result<Vec<ApiKeyRecord>, ApiKeyRepoError>;

    /// 鉴权路径:只返回未吊销的记录。
    async fn find_active(&self, id: &str) -> Result<Option<ApiKeyRecord>, ApiKeyRepoError>;

    /// 吊销(置 revoked=true)。返回 `false` 表示 id 不存在或已吊销。
    async fn revoke(&self, id: &str) -> Result<bool, ApiKeyRepoError>;
}
