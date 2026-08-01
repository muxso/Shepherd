//! RAG domain: documents, chunks, retrieval hits, the answer, and the decision-chain trace
//! (AskTrace / TraceStep) that the Q&A UI renders as a step tree/timeline.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagDocument {
    pub id: String,
    pub project_id: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A chunk of a document plus its embedding. `embedding` is empty on read paths that don't need it.
#[derive(Debug, Clone)]
pub struct RagChunk {
    pub id: String,
    pub document_id: String,
    pub project_id: String,
    pub chunk_index: i32,
    pub heading: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub created_at: i64,
}

/// A retrieval result (chunk + similarity score), joined with its document title.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub chunk_id: String,
    pub document_id: String,
    pub title: String,
    pub heading: String,
    pub content: String,
    pub score: f32,
}

/// One context slice fed to the LLM (indexed so the answer can cite `[index+1]`).
#[derive(Debug, Clone, Serialize)]
pub struct ContextChunk {
    pub index: usize,
    pub content: String,
    pub source_title: String,
    pub heading: String,
    pub document_id: String,
}

/// The synthesized answer plus the 1-based citation indices it referenced.
#[derive(Debug, Clone, Serialize)]
pub struct Answer {
    pub answer: String,
    pub cited_sources: Vec<usize>,
}

// ---- Decision chain (matches the frontend AskTrace/TraceStep shape) ----

#[derive(Debug, Clone, Serialize)]
pub struct TraceHit {
    pub topic: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceContextChunk {
    pub index: usize,
    pub topic: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TraceStep {
    Embedding { dim: usize, latency_ms: u64 },
    SemanticSearch { fetched: usize, top: Vec<TraceHit>, latency_ms: u64 },
    ContextBuilt { chunks: Vec<TraceContextChunk>, approx_tokens: usize },
    LlmGeneration { latency_ms: u64, answer_chars: usize },
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AskTrace {
    pub question: String,
    pub started_at: String,
    pub steps: Vec<TraceStep>,
    pub total_ms: u64,
    pub channel: &'static str,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RagError {
    #[error("rag storage error: {0}")]
    Backend(String),
    #[error("embedding error: {0}")]
    Embedding(String),
    #[error("llm error: {0}")]
    Llm(String),
    #[error("rag config error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, RagError>;
