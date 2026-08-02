//! IO adapters (feature `http`): OpenAI-compatible embeddings + chat over reqwest, and the
//! plain-Postgres vector store (real[] embeddings ranked by the in-DB `rag_cosine` function).

use async_trait::async_trait;
use serde_json::json;
use sqlx::PgPool;

use crate::domain::{Hit, RagChunk, RagDocument, RagError, Result};
use crate::ports::{Chat, Embedder, VectorStore};

// ---------- Embedder ----------

pub struct OpenAiEmbedder {
    client: reqwest::Client,
    url: String, // full /embeddings endpoint
    key: String,
    model: String,
    dim: usize,
}

impl OpenAiEmbedder {
    pub fn new(url: String, key: String, model: String, dim: usize) -> Self {
        Self { client: reqwest::Client::new(), url, key, model, dim }
    }
    /// Build from env: SHEPHERD_RAG_EMBED_URL/MODEL/DIM/KEY (KEY falls back to SHEPHERD_LLM_API_KEY).
    pub fn from_env() -> Self {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let url = env("SHEPHERD_RAG_EMBED_URL")
            .unwrap_or_else(|| "https://api.openai.com/v1/embeddings".to_string());
        let key = env("SHEPHERD_RAG_EMBED_KEY")
            .or_else(|| env("SHEPHERD_LLM_API_KEY"))
            .unwrap_or_default();
        let model =
            env("SHEPHERD_RAG_EMBED_MODEL").unwrap_or_else(|| "text-embedding-3-small".to_string());
        let dim = env("SHEPHERD_RAG_EMBED_DIM").and_then(|d| d.parse().ok()).unwrap_or(1536);
        Self::new(url, key, model, dim)
    }
}

#[async_trait]
impl Embedder for OpenAiEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(self
            .embed_batch(std::slice::from_ref(&text.to_string()))
            .await?
            .pop()
            .unwrap_or_default())
    }
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        if self.key.is_empty() {
            return Err(RagError::Config(
                "embedding API key not configured (SHEPHERD_RAG_EMBED_KEY)".into(),
            ));
        }
        // Volcano Engine multimodal embeddings take one {type:text} input at a time and return
        // { data: { embedding: [...] } } — a different shape from OpenAI's /embeddings batch.
        if self.url.contains("multimodal") {
            let mut out = Vec::with_capacity(texts.len());
            for t in texts {
                let resp = self
                    .client
                    .post(&self.url)
                    .bearer_auth(&self.key)
                    .json(&json!({ "model": self.model, "input": [{ "type": "text", "text": t }] }))
                    .send()
                    .await
                    .map_err(|e| RagError::Embedding(e.to_string()))?;
                if !resp.status().is_success() {
                    let s = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(RagError::Embedding(format!("embeddings HTTP {s}: {body}")));
                }
                let v: serde_json::Value =
                    resp.json().await.map_err(|e| RagError::Embedding(e.to_string()))?;
                let emb = v
                    .get("data")
                    .and_then(|d| d.get("embedding"))
                    .and_then(|e| e.as_array())
                    .ok_or_else(|| {
                        RagError::Embedding("multimodal response missing data.embedding[]".into())
                    })?;
                out.push(emb.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect());
            }
            return Ok(out);
        }
        let resp = self
            .client
            .post(&self.url)
            .bearer_auth(&self.key)
            .json(&json!({ "model": self.model, "input": texts }))
            .send()
            .await
            .map_err(|e| RagError::Embedding(e.to_string()))?;
        if !resp.status().is_success() {
            let s = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(RagError::Embedding(format!("embeddings HTTP {s}: {body}")));
        }
        let v: serde_json::Value =
            resp.json().await.map_err(|e| RagError::Embedding(e.to_string()))?;
        let data = v
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| RagError::Embedding("embeddings response missing data[]".into()))?;
        let mut out = Vec::with_capacity(data.len());
        for item in data {
            let emb = item
                .get("embedding")
                .and_then(|e| e.as_array())
                .ok_or_else(|| RagError::Embedding("embedding item missing embedding[]".into()))?;
            out.push(emb.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect());
        }
        Ok(out)
    }
}

// ---------- Chat ----------

pub struct OpenAiChat {
    client: reqwest::Client,
    url: String, // full /chat/completions endpoint
    key: String,
    model: String,
    max_tokens: u32,
}

impl OpenAiChat {
    pub fn from_env() -> Self {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let url = env("SHEPHERD_RAG_CHAT_URL")
            .or_else(|| env("SHEPHERD_LLM_URL"))
            .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string());
        let key = env("SHEPHERD_RAG_CHAT_KEY")
            .or_else(|| env("SHEPHERD_LLM_API_KEY"))
            .unwrap_or_default();
        let model = env("SHEPHERD_RAG_CHAT_MODEL")
            .or_else(|| env("SHEPHERD_LLM_MODEL"))
            .unwrap_or_else(|| "gpt-4o-mini".to_string());
        let max_tokens =
            env("SHEPHERD_RAG_MAX_TOKENS").and_then(|d| d.parse().ok()).unwrap_or(1500);
        Self { client: reqwest::Client::new(), url, key, model, max_tokens }
    }
}

#[async_trait]
impl Chat for OpenAiChat {
    async fn complete(&self, system: &str, user: &str) -> Result<String> {
        if self.key.is_empty() {
            return Err(RagError::Config(
                "LLM API key not configured (SHEPHERD_RAG_CHAT_KEY / SHEPHERD_LLM_API_KEY)".into(),
            ));
        }
        let resp = self
            .client
            .post(&self.url)
            .bearer_auth(&self.key)
            .json(&json!({
                "model": self.model,
                "max_tokens": self.max_tokens,
                "temperature": 0.0,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": user },
                ],
            }))
            .send()
            .await
            .map_err(|e| RagError::Llm(e.to_string()))?;
        if !resp.status().is_success() {
            let s = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(RagError::Llm(format!("chat HTTP {s}: {body}")));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| RagError::Llm(e.to_string()))?;
        v.get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| RagError::Llm("chat response missing choices[0].message.content".into()))
    }

    async fn complete_stream(
        &self,
        system: &str,
        user: &str,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<String> {
        use futures_util::StreamExt;
        if self.key.is_empty() {
            return Err(RagError::Config("LLM API key not configured".into()));
        }
        let resp = self
            .client
            .post(&self.url)
            .bearer_auth(&self.key)
            .json(&json!({
                "model": self.model,
                "max_tokens": self.max_tokens,
                "temperature": 0.0,
                "stream": true,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": user },
                ],
            }))
            .send()
            .await
            .map_err(|e| RagError::Llm(e.to_string()))?;
        if !resp.status().is_success() {
            let s = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(RagError::Llm(format!("chat HTTP {s}: {body}")));
        }
        // Parse the OpenAI SSE stream: `data: {choices:[{delta:{content}}]}` lines, `data: [DONE]` ends it.
        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        let mut full = String::new();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| RagError::Llm(e.to_string()))?;
            buf.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(nl) = buf.find('\n') {
                let line: String = buf.drain(..=nl).collect();
                let line = line.trim();
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(c) = v
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        if !c.is_empty() {
                            full.push_str(c);
                            if tx.send(c.to_string()).await.is_err() {
                                break; // receiver dropped (client disconnected)
                            }
                        }
                    }
                }
            }
        }
        Ok(full)
    }
}

// ---------- Vector store (plain Postgres) ----------

#[derive(Clone)]
pub struct PgVectorStore {
    pool: PgPool,
    keyword: Option<std::sync::Arc<crate::tantivy_kw::TantivyKeyword>>,
}

impl PgVectorStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, keyword: None }
    }
    /// Attach a tantivy keyword index (jieba full-text). Without it, keyword search uses PG ILIKE.
    pub fn with_keyword(
        pool: PgPool,
        keyword: std::sync::Arc<crate::tantivy_kw::TantivyKeyword>,
    ) -> Self {
        Self { pool, keyword: Some(keyword) }
    }
}

fn be(e: sqlx::Error) -> RagError {
    RagError::Backend(e.to_string())
}

/// Rebuild the tantivy keyword index from all stored chunks (called once on startup so the RAM
/// index matches the DB). Returns the number of chunks indexed.
pub async fn rebuild_keyword_index(
    pool: &PgPool,
    kw: &crate::tantivy_kw::TantivyKeyword,
) -> Result<usize> {
    let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT c.id, c.document_id, c.project_id, c.heading, c.content FROM ms_rag_chunk c",
    )
    .fetch_all(pool)
    .await
    .map_err(be)?;
    let n = rows.len();
    kw.bulk_add(&rows)?;
    Ok(n)
}

#[async_trait]
impl VectorStore for PgVectorStore {
    async fn upsert_document(&self, doc: &RagDocument) -> Result<()> {
        sqlx::query(
            "INSERT INTO ms_rag_document (id, project_id, source_type, source_id, title, created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7) \
             ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, source_type = EXCLUDED.source_type, \
                source_id = EXCLUDED.source_id, updated_at = EXCLUDED.updated_at",
        )
        .bind(&doc.id)
        .bind(&doc.project_id)
        .bind(&doc.source_type)
        .bind(&doc.source_id)
        .bind(&doc.title)
        .bind(doc.created_at)
        .bind(doc.updated_at)
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }

    async fn replace_chunks(&self, document_id: &str, chunks: &[RagChunk]) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(be)?;
        sqlx::query("DELETE FROM ms_rag_chunk WHERE document_id = $1")
            .bind(document_id)
            .execute(&mut *tx)
            .await
            .map_err(be)?;
        for c in chunks {
            sqlx::query(
                "INSERT INTO ms_rag_chunk (id, document_id, project_id, chunk_index, heading, content, embedding, created_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(&c.id)
            .bind(&c.document_id)
            .bind(&c.project_id)
            .bind(c.chunk_index)
            .bind(&c.heading)
            .bind(&c.content)
            .bind(&c.embedding)
            .bind(c.created_at)
            .execute(&mut *tx)
            .await
            .map_err(be)?;
        }
        tx.commit().await.map_err(be)?;
        if let Some(kw) = &self.keyword {
            let rows: Vec<(String, String, String, String)> = chunks
                .iter()
                .map(|c| (c.id.clone(), c.project_id.clone(), c.heading.clone(), c.content.clone()))
                .collect();
            kw.replace_doc(document_id, &rows)?;
        }
        Ok(())
    }

    async fn delete_document(&self, id: &str) -> Result<()> {
        // Chunks cascade in PG; the tantivy index keys on document_id so delete there too.
        if let Some(kw) = &self.keyword {
            let _ = kw.delete_doc(id);
        }
        sqlx::query("DELETE FROM ms_rag_document WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(be)?;
        Ok(())
    }

    async fn search(&self, project_id: &str, query: &[f32], top_k: usize) -> Result<Vec<Hit>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, f64)>(
            "SELECT c.id, c.document_id, COALESCE(d.title,''), c.heading, c.content, \
                    rag_cosine(c.embedding, $2) AS score \
             FROM ms_rag_chunk c JOIN ms_rag_document d ON d.id = c.document_id \
             WHERE c.project_id = $1 \
             ORDER BY score DESC LIMIT $3",
        )
        .bind(project_id)
        .bind(query)
        .bind(top_k as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        Ok(rows
            .into_iter()
            .map(|(chunk_id, document_id, title, heading, content, score)| Hit {
                chunk_id,
                document_id,
                title,
                heading,
                content,
                score: score as f32,
            })
            .collect())
    }

    async fn keyword_search(
        &self,
        project_id: &str,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<Hit>> {
        // Tantivy (jieba full-text) when available: rank chunk ids, then hydrate them from PG.
        if let Some(kw) = &self.keyword {
            let ranked = kw.search(project_id, query, top_k)?;
            if ranked.is_empty() {
                return Ok(Vec::new());
            }
            let ids: Vec<String> = ranked.iter().map(|(id, _)| id.clone()).collect();
            let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
                "SELECT c.id, c.document_id, COALESCE(d.title,''), c.heading, c.content \
                 FROM ms_rag_chunk c JOIN ms_rag_document d ON d.id = c.document_id \
                 WHERE c.id = ANY($1)",
            )
            .bind(&ids)
            .fetch_all(&self.pool)
            .await
            .map_err(be)?;
            let by_id: std::collections::HashMap<String, (String, String, String, String)> = rows
                .into_iter()
                .map(|(id, doc, title, heading, content)| (id, (doc, title, heading, content)))
                .collect();
            return Ok(ranked
                .into_iter()
                .filter_map(|(id, score)| {
                    by_id.get(&id).map(|(doc, title, heading, content)| Hit {
                        chunk_id: id.clone(),
                        document_id: doc.clone(),
                        title: title.clone(),
                        heading: heading.clone(),
                        content: content.clone(),
                        score,
                    })
                })
                .collect());
        }
        // Fallback: whitespace terms → ILIKE patterns; score = how many distinct terms a chunk contains.
        let terms: Vec<String> = query
            .split_whitespace()
            .filter(|t| t.chars().count() >= 2)
            .take(12)
            .map(|t| format!("%{}%", t.replace(['%', '_'], "")))
            .collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_as::<_, (String, String, String, String, String, f64)>(
            "SELECT c.id, c.document_id, COALESCE(d.title,''), c.heading, c.content, \
                    (SELECT count(*) FROM unnest($2::text[]) p WHERE c.content ILIKE p)::float8 AS score \
             FROM ms_rag_chunk c JOIN ms_rag_document d ON d.id = c.document_id \
             WHERE c.project_id = $1 AND c.content ILIKE ANY($2::text[]) \
             ORDER BY score DESC LIMIT $3",
        )
        .bind(project_id)
        .bind(&terms)
        .bind(top_k as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        Ok(rows
            .into_iter()
            .map(|(chunk_id, document_id, title, heading, content, score)| Hit {
                chunk_id,
                document_id,
                title,
                heading,
                content,
                score: score as f32,
            })
            .collect())
    }
}
