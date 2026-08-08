//! Ports: the RAG pipeline depends on these traits, not on concrete HTTP/DB/LLM clients.

use async_trait::async_trait;

use crate::domain::{Audience, Hit, RagChunk, RagDocument, Result};

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
    /// Chunks stored without an embedding (keyword-only ingests), for backfill. `embedding` is empty
    /// on the returned chunks; only id/heading/content are needed to re-embed. Default: none.
    async fn chunks_missing_embedding(
        &self,
        _project_id: Option<&str>,
        _limit: usize,
    ) -> Result<Vec<RagChunk>> {
        Ok(Vec::new())
    }
    /// Persist backfilled embeddings by chunk id. Returns how many rows were updated. Default: no-op.
    async fn set_chunk_embeddings(&self, _updates: &[(String, Vec<f32>)]) -> Result<usize> {
        Ok(0)
    }
    /// Top-k chunks in `project_id` by cosine similarity to `query`, filtered to what `audience` may see.
    async fn search(
        &self,
        project_id: &str,
        query: &[f32],
        top_k: usize,
        audience: &Audience,
    ) -> Result<Vec<Hit>>;
    /// Top-k chunks by keyword match (for hybrid retrieval), filtered to what `audience` may see.
    /// Default: none (semantic-only store).
    async fn keyword_search(
        &self,
        _project_id: &str,
        _query: &str,
        _top_k: usize,
        _audience: &Audience,
    ) -> Result<Vec<Hit>> {
        Ok(Vec::new())
    }
}

/// Synthesizes an answer from a system + user prompt (an OpenAI-compatible chat client, or a fake).
#[async_trait]
pub trait Chat: Send + Sync {
    async fn complete(&self, system: &str, user: &str) -> Result<String>;

    /// Stream the answer token-by-token: each delta is sent on `tx`; returns the full text.
    /// Default: falls back to `complete` and emits it as a single delta.
    #[cfg(feature = "http")]
    async fn complete_stream(
        &self,
        system: &str,
        user: &str,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<String> {
        let full = self.complete(system, user).await?;
        let _ = tx.send(full.clone()).await;
        Ok(full)
    }
}
