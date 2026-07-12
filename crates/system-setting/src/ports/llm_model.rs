use async_trait::async_trait;
use thiserror::Error;

/// Per-user LLM model config. api_key is stored in plaintext for backend calls; read
/// endpoints only ever return a masked value.
#[derive(Debug, Clone)]
pub struct LlmModelRecord {
    pub id: String,
    pub user_id: String,
    /// deepseek/openai/zhipu/custom etc., lowercased before storage.
    pub provider: String,
    /// Model name, e.g. deepseek-chat.
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub enabled: bool,
    pub created_at_ms: i64,
}

/// Partial update; None = leave the field unchanged.
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
    /// Same user+provider+name → Duplicate.
    async fn insert(
        &self,
        user_id: &str,
        provider: &str,
        name: &str,
        base_url: &str,
        api_key: &str,
    ) -> Result<LlmModelRecord, LlmModelRepoError>;

    async fn list_by_user(&self, user_id: &str) -> Result<Vec<LlmModelRecord>, LlmModelRepoError>;

    /// Only updates the caller's own rows; returns None if missing or not owned. Renaming
    /// into an existing unique key → Duplicate.
    async fn update(
        &self,
        user_id: &str,
        id: &str,
        patch: LlmModelPatch,
    ) -> Result<Option<LlmModelRecord>, LlmModelRepoError>;

    /// Only deletes the caller's own rows; `false` means missing or not owned.
    async fn delete(&self, user_id: &str, id: &str) -> Result<bool, LlmModelRepoError>;
}
