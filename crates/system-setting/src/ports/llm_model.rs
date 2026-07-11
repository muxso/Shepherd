use async_trait::async_trait;
use thiserror::Error;

/// 个人 LLM 模型配置。api_key 存原文供后端调用,读接口一律只回掩码。
#[derive(Debug, Clone)]
pub struct LlmModelRecord {
    pub id: String,
    pub user_id: String,
    /// deepseek/openai/zhipu/custom 等,入库前已小写。
    pub provider: String,
    /// 模型名,如 deepseek-chat。
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub enabled: bool,
    pub created_at_ms: i64,
}

/// 局部更新;None = 该字段不动。
#[derive(Debug, Clone, Default)]
pub struct LlmModelPatch {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LlmModelRepoError {
    #[error("model already exists for this user/provider/name")]
    Duplicate,
    #[error("llm model backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait LlmModelRepository: Send + Sync {
    /// 同 user+provider+name 重复 → Duplicate。
    async fn insert(
        &self,
        user_id: &str,
        provider: &str,
        name: &str,
        base_url: &str,
        api_key: &str,
    ) -> Result<LlmModelRecord, LlmModelRepoError>;

    async fn list_by_user(&self, user_id: &str) -> Result<Vec<LlmModelRecord>, LlmModelRepoError>;

    /// 只更新本人的行;不存在或非本人返回 None。改名撞唯一键 → Duplicate。
    async fn update(
        &self,
        user_id: &str,
        id: &str,
        patch: LlmModelPatch,
    ) -> Result<Option<LlmModelRecord>, LlmModelRepoError>;

    /// 只删本人的行;`false` 表示不存在或非本人。
    async fn delete(&self, user_id: &str, id: &str) -> Result<bool, LlmModelRepoError>;
}
