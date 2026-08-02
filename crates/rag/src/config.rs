//! Runtime RAG configuration (embedding + chat endpoints/models/keys), held behind an RwLock so
//! the settings admin can hot-swap it without a restart. Env (`SHEPHERD_RAG_*`) is the fallback.

use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct RagConfig {
    pub embed_url: String,
    pub embed_model: String,
    pub embed_dim: usize,
    pub embed_key: String,
    pub chat_url: String,
    pub chat_model: String,
    pub chat_key: String,
    pub max_tokens: u32,
    pub top_k: usize,
    pub rerank: bool,
}

/// Shared, hot-swappable config. Readers clone the guard's value out and drop the lock before I/O.
pub type RagConfigHandle = Arc<RwLock<RagConfig>>;

impl RagConfig {
    pub fn from_env() -> Self {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let llm_key = env("SHEPHERD_LLM_API_KEY");
        Self {
            embed_url: env("SHEPHERD_RAG_EMBED_URL")
                .unwrap_or_else(|| "https://api.openai.com/v1/embeddings".into()),
            embed_model: env("SHEPHERD_RAG_EMBED_MODEL")
                .unwrap_or_else(|| "text-embedding-3-small".into()),
            embed_dim: env("SHEPHERD_RAG_EMBED_DIM").and_then(|d| d.parse().ok()).unwrap_or(1536),
            embed_key: env("SHEPHERD_RAG_EMBED_KEY")
                .or_else(|| llm_key.clone())
                .unwrap_or_default(),
            chat_url: env("SHEPHERD_RAG_CHAT_URL")
                .or_else(|| env("SHEPHERD_LLM_URL"))
                .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".into()),
            chat_model: env("SHEPHERD_RAG_CHAT_MODEL")
                .or_else(|| env("SHEPHERD_LLM_MODEL"))
                .unwrap_or_else(|| "gpt-4o-mini".into()),
            chat_key: env("SHEPHERD_RAG_CHAT_KEY").or(llm_key).unwrap_or_default(),
            max_tokens: env("SHEPHERD_RAG_MAX_TOKENS").and_then(|d| d.parse().ok()).unwrap_or(1500),
            top_k: 8,
            rerank: true,
        }
    }

    pub fn handle_from_env() -> RagConfigHandle {
        Arc::new(RwLock::new(Self::from_env()))
    }
}
