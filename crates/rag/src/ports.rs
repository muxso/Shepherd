//! Ports: the RAG pipeline depends on these traits, not on concrete HTTP/DB/LLM clients.

use async_trait::async_trait;

use crate::domain::{Hit, RagChunk, RagDocument, Result};

/// Turns text into embedding vectors (an OpenAI-compatible `/embeddings` client, or a fake).
#[async_trait]
pub trait Embedder: Send + Sync {
    fn dim(&self) -> usize;
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            out.push(self.embed(t).await?);
        }
        Ok(out)
    }
}

/// Where document chunks + embeddings live and are searched (pg real[] cosine, or pgvector later).
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert_document(&self, doc: &RagDocument) -> Result<()>;
    async fn replace_chunks(&self, document_id: &str, chunks: &[RagChunk]) -> Result<()>;
    async fn delete_document(&self, id: &str) -> Result<()>;
    /// Top-k chunks in `project_id` by cosine similarity to `query`.
    async fn search(&self, project_id: &str, query: &[f32], top_k: usize) -> Result<Vec<Hit>>;
    /// Top-k chunks by keyword match (for hybrid retrieval). Default: none (semantic-only store).
    async fn keyword_search(
        &self,
        _project_id: &str,
        _query: &str,
        _top_k: usize,
    ) -> Result<Vec<Hit>> {
        Ok(Vec::new())
    }
}

/// Synthesizes an answer from a system + user prompt (an OpenAI-compatible chat client, or a fake).
#[async_trait]
pub trait Chat: Send + Sync {
    async fn complete(&self, system: &str, user: &str) -> Result<String>;
}
