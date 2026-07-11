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
    /// 属主用户 id;管理端旧口径的 key 无属主(空串)。
    pub user_id: String,
    /// 过期时刻(epoch 毫秒);None = 永久。
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

    /// 个人视角:只列属主为 user_id 的 key。
    async fn list_by_user(&self, user_id: &str) -> Result<Vec<ApiKeyRecord>, ApiKeyRepoError>;

    /// 管理路径:按 id 取记录,不过滤 revoked/过期(属主判定用)。
    async fn find(&self, id: &str) -> Result<Option<ApiKeyRecord>, ApiKeyRepoError>;

    /// 鉴权路径:只返回未吊销且未过期的记录。
    async fn find_active(&self, id: &str) -> Result<Option<ApiKeyRecord>, ApiKeyRepoError>;

    /// 吊销(置 revoked=true)。返回 `false` 表示 id 不存在或已吊销。
    async fn revoke(&self, id: &str) -> Result<bool, ApiKeyRepoError>;

    /// 启停:enabled=false 等价吊销,true 恢复;幂等。返回 `false` 表示 id 不存在。
    async fn set_enabled(&self, id: &str, enabled: bool) -> Result<bool, ApiKeyRepoError>;
}
