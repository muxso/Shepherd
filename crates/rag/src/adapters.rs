//! IO adapters (feature `http`): OpenAI-compatible embeddings + chat over reqwest, and the
//! plain-Postgres vector store (real[] embeddings ranked by the in-DB `rag_cosine` function).

use async_trait::async_trait;
use serde_json::json;
use serde_json::Value;
use sqlx::PgPool;

use crate::domain::{Audience, Hit, RagChunk, RagDocument, RagError, Result};
use crate::ports::{Chat, Embedder, VectorStore};

fn embedding_http_error(status: reqwest::StatusCode, body: &str) -> RagError {
    if status == reqwest::StatusCode::BAD_REQUEST
        && body.to_ascii_lowercase().contains("input length exceeds the context length")
    {
        RagError::EmbeddingInputTooLong
    } else {
        RagError::Embedding(format!("embeddings HTTP {status}: {body}"))
    }
}

// ---------- Embedder ----------

pub struct OpenAiEmbedder {
    client: reqwest::Client,
    cfg: crate::config::RagConfigHandle,
}

impl OpenAiEmbedder {
    pub fn new(cfg: crate::config::RagConfigHandle) -> Self {
        Self { client: reqwest::Client::new(), cfg }
    }
    pub fn from_env() -> Self {
        Self::new(crate::config::RagConfig::handle_from_env())
    }
    fn snapshot(&self) -> (String, String, String) {
        let c = self.cfg.read().expect("rag config poisoned");
        (c.embed_url.clone(), c.embed_key.clone(), c.embed_model.clone())
    }
}

#[async_trait]
impl Embedder for OpenAiEmbedder {
    fn dim(&self) -> usize {
        self.cfg.read().expect("rag config poisoned").embed_dim
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
        let (url, key, model) = self.snapshot();
        if key.is_empty() {
            return Err(RagError::Config(
                "embedding API key not configured (System Params → RAG config)".into(),
            ));
        }
        // Volcano Engine multimodal embeddings take one {type:text} input at a time and return
        // { data: { embedding: [...] } } — a different shape from OpenAI's /embeddings batch.
        if url.contains("multimodal") {
            let mut out = Vec::with_capacity(texts.len());
            for t in texts {
                let resp = self
                    .client
                    .post(&url)
                    .bearer_auth(&key)
                    .json(&json!({ "model": model, "input": [{ "type": "text", "text": t }] }))
                    .send()
                    .await
                    .map_err(|e| RagError::Embedding(e.to_string()))?;
                if !resp.status().is_success() {
                    let s = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(embedding_http_error(s, &body));
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
            .post(&url)
            .bearer_auth(&key)
            .json(&json!({ "model": model, "input": texts }))
            .send()
            .await
            .map_err(|e| RagError::Embedding(e.to_string()))?;
        if !resp.status().is_success() {
            let s = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(embedding_http_error(s, &body));
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

fn is_ollama_chat(url: &str) -> bool {
    url.trim_end_matches('/').ends_with("/api/chat")
}

/// Native Ollama uses NDJSON, explicit thinking controls and an options token budget.
fn chat_request(
    url: &str,
    model: &str,
    max_tokens: u32,
    system: &str,
    user: &str,
    stream: bool,
) -> serde_json::Value {
    let messages = json!([
        {"role": "system", "content": system},
        {"role": "user", "content": user},
    ]);
    if is_ollama_chat(url) {
        json!({"model": model, "messages": messages, "stream": stream, "think": false,
            "options": {"num_predict": max_tokens, "temperature": 0.0}})
    } else {
        json!({"model": model, "messages": messages, "stream": stream,
            "max_tokens": max_tokens, "temperature": 0.0})
    }
}

// ---------- Chat ----------

pub struct OpenAiChat {
    client: reqwest::Client,
    cfg: crate::config::RagConfigHandle,
}

impl OpenAiChat {
    pub fn new(cfg: crate::config::RagConfigHandle) -> Self {
        Self { client: reqwest::Client::new(), cfg }
    }
    pub fn from_env() -> Self {
        Self::new(crate::config::RagConfig::handle_from_env())
    }
    fn snapshot(&self) -> (String, String, String, u32) {
        let c = self.cfg.read().expect("rag config poisoned");
        (c.chat_url.clone(), c.chat_key.clone(), c.chat_model.clone(), c.max_tokens)
    }
}

#[async_trait]
impl Chat for OpenAiChat {
    async fn complete(&self, system: &str, user: &str) -> Result<String> {
        let (url, key, model, max_tokens) = self.snapshot();
        if key.is_empty() && !is_ollama_chat(&url) {
            return Err(RagError::Config(
                "LLM API key not configured (System Params → RAG config)".into(),
            ));
        }
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&key)
            .json(&chat_request(&url, &model, max_tokens, system, user, false))
            .send()
            .await
            .map_err(|e| RagError::Llm(e.to_string()))?;
        if !resp.status().is_success() {
            let s = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(RagError::Llm(format!("chat HTTP {s}: {body}")));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| RagError::Llm(e.to_string()))?;
        if is_ollama_chat(&url) {
            if let Some(error) = v.get("error").and_then(|e| e.as_str()) {
                return Err(RagError::Llm(error.into()));
            }
            return v
                .pointer("/message/content")
                .and_then(|c| c.as_str())
                .map(str::to_owned)
                .ok_or_else(|| RagError::Llm("Ollama response missing message.content".into()));
        }
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
        let (url, key, model, max_tokens) = self.snapshot();
        if key.is_empty() && !is_ollama_chat(&url) {
            return Err(RagError::Config(
                "LLM API key not configured (System Params → RAG config)".into(),
            ));
        }
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&key)
            .json(&chat_request(&url, &model, max_tokens, system, user, true))
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
        let mut buf = Vec::<u8>::new();
        let mut full = String::new();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| RagError::Llm(e.to_string()))?;
            buf.extend_from_slice(&bytes);
            while let Some(nl) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=nl).collect();
                let line = std::str::from_utf8(&line).map_err(|e| RagError::Llm(e.to_string()))?;
                let line = line.trim();
                let data = if is_ollama_chat(&url) {
                    line
                } else {
                    let Some(data) = line.strip_prefix("data:") else { continue };
                    data.trim()
                };
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(error) = v.get("error").and_then(|e| e.as_str()) {
                        return Err(RagError::Llm(error.into()));
                    }
                    let content = if is_ollama_chat(&url) {
                        v.pointer("/message/content")
                    } else {
                        v.pointer("/choices/0/delta/content")
                    };
                    if let Some(c) = content.and_then(|c| c.as_str()) {
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
        if full.trim().is_empty() {
            return Err(RagError::Llm("Model returned no answer content".into()));
        }
        Ok(full)
    }
}

// ---------- Vector store (plain Postgres) ----------

#[derive(Clone)]
pub struct PgVectorStore {
    pool: PgPool,
    keyword: Option<std::sync::Arc<crate::tantivy_kw::TantivyKeyword>>,
    ann: Option<crate::qdrant::QdrantIndex>,
}

impl PgVectorStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, keyword: None, ann: crate::qdrant::QdrantIndex::from_env() }
    }
    /// Attach a tantivy keyword index (jieba full-text). Without it, keyword search uses PG ILIKE.
    pub fn with_keyword(
        pool: PgPool,
        keyword: std::sync::Arc<crate::tantivy_kw::TantivyKeyword>,
    ) -> Self {
        Self { pool, keyword: Some(keyword), ann: crate::qdrant::QdrantIndex::from_env() }
    }
}

fn be(e: sqlx::Error) -> RagError {
    RagError::Backend(e.to_string())
}

/// SQL predicate (over joined document alias `d`) limiting rows to what the audience may retrieve:
/// admin bypass, or the uploader, or an overlap between the doc's `visibility_groups` and the groups
/// the caller can see. Positional params `$admin` (bool), `$user` (text, nullable), `$groups` (text[])
/// must be bound in that order.
fn audience_where(admin: usize, user: usize, groups: usize) -> String {
    format!(
        "(${admin} OR (d.owner_id IS NOT NULL AND d.owner_id = ${user}) \
          OR d.visibility_groups && ${groups}::text[])"
    )
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
        let metadata: Value = doc
            .source_id
            .as_deref()
            .and_then(|v| serde_json::from_str(v).ok())
            .unwrap_or(Value::Null);
        let code = doc.source_type == "code";
        sqlx::query(
            "INSERT INTO ms_rag_document \
                (id, project_id, source_type, source_id, title, owner_id, visibility_groups, created_at, updated_at,code_repository_id,code_branch,code_path,code_commit,code_blob) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) \
             ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, source_type = EXCLUDED.source_type, \
                source_id = EXCLUDED.source_id, visibility_groups = EXCLUDED.visibility_groups, \
                owner_id = COALESCE(ms_rag_document.owner_id, EXCLUDED.owner_id), \
                updated_at = EXCLUDED.updated_at,code_repository_id=EXCLUDED.code_repository_id,code_branch=EXCLUDED.code_branch,code_path=EXCLUDED.code_path,code_commit=EXCLUDED.code_commit,code_blob=EXCLUDED.code_blob",
        )
        .bind(&doc.id)
        .bind(&doc.project_id)
        .bind(&doc.source_type)
        .bind(&doc.source_id)
        .bind(&doc.title)
        .bind(&doc.owner_id)
        .bind(&doc.visibility_groups)
        .bind(doc.created_at)
        .bind(doc.updated_at)
        .bind(if code {metadata["repositoryId"].as_str()}else{None})
        .bind(if code {metadata["branch"].as_str()}else{None})
        .bind(if code {metadata["path"].as_str()}else{None})
        .bind(if code {metadata["commit"].as_str()}else{None})
        .bind(if code {metadata["blob"].as_str()}else{None})
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
        if let Some(ann) = &self.ann {
            ann.delete_document(document_id).await?;
            ann.upsert(chunks).await?;
        }
        if let Some(kw) = &self.keyword {
            let rows: Vec<(String, String, String, String)> = chunks
                .iter()
                .map(|c| (c.id.clone(), c.project_id.clone(), c.heading.clone(), c.content.clone()))
                .collect();
            kw.replace_doc(document_id, &rows)?;
        }
        Ok(())
    }

    async fn chunks_missing_embedding(
        &self,
        project_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RagChunk>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
            "SELECT id, document_id, project_id, heading, content FROM ms_rag_chunk \
             WHERE cardinality(embedding) = 0 AND ($1::text IS NULL OR project_id = $1) \
             ORDER BY created_at LIMIT $2",
        )
        .bind(project_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        Ok(rows
            .into_iter()
            .map(|(id, document_id, project_id, heading, content)| RagChunk {
                id,
                document_id,
                project_id,
                chunk_index: 0,
                heading,
                content,
                embedding: Vec::new(),
                created_at: 0,
            })
            .collect())
    }

    async fn set_chunk_embeddings(&self, updates: &[(String, Vec<f32>)]) -> Result<usize> {
        let mut tx = self.pool.begin().await.map_err(be)?;
        let mut n = 0;
        for (id, emb) in updates {
            let r = sqlx::query("UPDATE ms_rag_chunk SET embedding = $1 WHERE id = $2")
                .bind(emb)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(be)?;
            n += r.rows_affected() as usize;
        }
        tx.commit().await.map_err(be)?;
        if let Some(ann) = &self.ann {
            let ids: Vec<&str> = updates.iter().map(|(id, _)| id.as_str()).collect();
            let rows = sqlx::query_as::<_, (String, String, String, Vec<f32>)>(
                "SELECT id, document_id, project_id, embedding FROM ms_rag_chunk WHERE id = ANY($1)",
            ).bind(ids).fetch_all(&self.pool).await.map_err(be)?;
            let chunks: Vec<RagChunk> = rows
                .into_iter()
                .map(|(id, document_id, project_id, embedding)| RagChunk {
                    id,
                    document_id,
                    project_id,
                    embedding,
                    chunk_index: 0,
                    heading: String::new(),
                    content: String::new(),
                    created_at: 0,
                })
                .collect();
            ann.upsert(&chunks).await?;
        }
        Ok(n)
    }

    async fn delete_document(&self, id: &str) -> Result<()> {
        if let Some(ann) = &self.ann {
            ann.delete_document(id).await?;
        }
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

    async fn search(
        &self,
        project_id: &str,
        query: &[f32],
        top_k: usize,
        audience: &Audience,
    ) -> Result<Vec<Hit>> {
        if let Some(ann) = &self.ann {
            let ranked = ann.search(project_id, query, (top_k * 16).clamp(256, 1000)).await?;
            let ids: Vec<&str> = ranked.iter().map(|(id, _)| id.as_str()).collect();
            // Never trust side-index payload for authorization. Re-check current project and ACLs.
            let rows = sqlx::query_as::<_, (String, String, String, String, String)>(&format!(
                "SELECT c.id, c.document_id, d.title, c.heading, c.content FROM ms_rag_chunk c \
                 JOIN ms_rag_document d ON d.id=c.document_id \
                 WHERE c.id = ANY($1) AND {} AND c.project_id=$5 AND d.project_id=$5",
                audience_where(2, 3, 4)
            ))
            .bind(ids)
            .bind(audience.is_admin)
            .bind(&audience.user_id)
            .bind(&audience.visible_group_ids)
            .bind(project_id)
            .fetch_all(&self.pool)
            .await
            .map_err(be)?;
            let by_id: std::collections::HashMap<_, _> = rows
                .into_iter()
                .map(|(id, doc, title, heading, content)| (id, (doc, title, heading, content)))
                .collect();
            return Ok(ranked
                .into_iter()
                .filter_map(|(id, score)| {
                    by_id.get(&id).map(|(doc, title, heading, content)| Hit {
                        chunk_id: id,
                        document_id: doc.clone(),
                        title: title.clone(),
                        heading: heading.clone(),
                        content: content.clone(),
                        score,
                    })
                })
                .take(top_k)
                .collect());
        }
        let rows = sqlx::query_as::<_, (String, String, String, String, String, f64)>(&format!(
            "SELECT c.id, c.document_id, COALESCE(d.title,''), c.heading, c.content, \
                    rag_cosine(c.embedding, $2) AS score \
             FROM ms_rag_chunk c JOIN ms_rag_document d ON d.id = c.document_id \
             WHERE c.project_id = $1 AND cardinality(c.embedding) > 0 AND {} \
             ORDER BY score DESC LIMIT $3",
            audience_where(4, 5, 6)
        ))
        .bind(project_id)
        .bind(query)
        .bind(top_k as i64)
        .bind(audience.is_admin)
        .bind(&audience.user_id)
        .bind(&audience.visible_group_ids)
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

    async fn activity_search(
        &self,
        project_id: &str,
        question: &str,
        audience: &Audience,
    ) -> Result<Option<Vec<Hit>>> {
        self.activity_search_at(project_id, question, audience, crate::application::now_ms()).await
    }
    async fn activity_search_at(
        &self,
        project_id: &str,
        question: &str,
        audience: &Audience,
        at_ms: i64,
    ) -> Result<Option<Vec<Hit>>> {
        use crate::activity::{
            git_change_mentioned, team_mentioned, ActivityMetadata, ActivityWindow,
        };
        let Some(window) = ActivityWindow::parse(question, at_ms) else {
            return Ok(None);
        };
        let sql=format!("SELECT d.id,d.source_id FROM ms_rag_document d WHERE d.project_id=$1 AND d.source_type='git_activity' AND {}",audience_where(2,3,4));
        let rows = sqlx::query_as::<_, (String, Option<String>)>(&sql)
            .bind(project_id)
            .bind(audience.is_admin)
            .bind(&audience.user_id)
            .bind(&audience.visible_group_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(be)?;
        let metadata: Vec<_> = rows
            .into_iter()
            .filter_map(|(id, raw)| {
                raw.and_then(|r| serde_json::from_str::<ActivityMetadata>(&r).ok()).map(|m| (id, m))
            })
            .collect();
        let author_named = metadata.iter().any(|(_, m)| m.matches_author(question));
        let explicit_activity = git_change_mentioned(question)
            || [
                "干了",
                "干嘛",
                "干什么",
                "干啥",
                "做了",
                "做什么",
                "做啥",
                "改了",
                "git log",
                "提交记录",
                "提交历史",
            ]
            .iter()
            .any(|s| question.to_lowercase().contains(s));
        if !author_named && !team_mentioned(question) && !explicit_activity {
            return Ok(None);
        }
        let mut selected = Vec::new();
        for (id, meta) in metadata {
            // Each multipart day repeats its global summary. Select that summary once;
            // remaining evidence parts remain available through ordinary source retrieval.
            if meta.part.unwrap_or(1) == 1
                && window.includes(&meta.date)
                && (meta.matches_author(question) || (!author_named && team_mentioned(question)))
            {
                selected.push((meta.date, id));
            }
        }
        selected.sort();
        selected.reverse();
        let matched = selected.len();
        selected.truncate(31);
        let ids: Vec<_> = selected.into_iter().map(|(_, id)| id).collect();
        if ids.is_empty() {
            return Ok(Some(Vec::new()));
        }
        // Recheck audience when hydrating, even if access changed since the metadata lookup.
        let sql=format!("SELECT d.id,d.title,c.id,c.heading,c.content FROM ms_rag_document d JOIN LATERAL (SELECT id,heading,content,chunk_index FROM ms_rag_chunk WHERE document_id=d.id ORDER BY chunk_index LIMIT 40) c ON true WHERE d.project_id=$1 AND d.id=ANY($2::text[]) AND {} ORDER BY array_position($2::text[],d.id),c.chunk_index",audience_where(3,4,5));
        let rows = sqlx::query_as::<_, (String, String, String, String, String)>(&sql)
            .bind(project_id)
            .bind(&ids)
            .bind(audience.is_admin)
            .bind(&audience.user_id)
            .bind(&audience.visible_group_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(be)?;
        let cap = (8000 / ids.len()).max(100);
        let mut hits: Vec<Hit> = Vec::new();
        for (id, title, chunk_id, heading, content) in rows {
            if let Some(hit) = hits.last_mut().filter(|h| h.document_id == id) {
                let remaining = cap.saturating_sub(hit.content.chars().count());
                if remaining > 0 {
                    hit.content.push('\n');
                    hit.content.extend(content.chars().take(remaining));
                }
            } else {
                hits.push(Hit{document_id:id,chunk_id,title,heading,content:format!("检索范围 {} 至 {}（上海），共匹配 {} 条作者日摘要，本次列出最多31条；大记录可能分段，此处仅列摘要与部分明细。只表示已索引Git提交，不代表全部工作。\n{}",window.from,window.through,matched,content.chars().take(cap).collect::<String>()),score:1.0});
            }
        }
        Ok(Some(hits))
    }

    async fn keyword_search(
        &self,
        project_id: &str,
        query: &str,
        top_k: usize,
        audience: &Audience,
    ) -> Result<Vec<Hit>> {
        // Tantivy (jieba full-text) when available: rank chunk ids, then hydrate them from PG.
        if let Some(kw) = &self.keyword {
            // Over-fetch: the audience filter runs after tantivy ranking, so restricted hits are dropped.
            let ranked = kw.search(project_id, query, top_k)?;
            if ranked.is_empty() {
                return Ok(Vec::new());
            }
            let ids: Vec<String> = ranked.iter().map(|(id, _)| id.clone()).collect();
            let rows = sqlx::query_as::<_, (String, String, String, String, String)>(&format!(
                "SELECT c.id, c.document_id, COALESCE(d.title,''), c.heading, c.content \
                 FROM ms_rag_chunk c JOIN ms_rag_document d ON d.id = c.document_id \
                 WHERE c.id = ANY($1) AND {}",
                audience_where(2, 3, 4)
            ))
            .bind(&ids)
            .bind(audience.is_admin)
            .bind(&audience.user_id)
            .bind(&audience.visible_group_ids)
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
        let rows = sqlx::query_as::<_, (String, String, String, String, String, f64)>(&format!(
            "SELECT c.id, c.document_id, COALESCE(d.title,''), c.heading, c.content, \
                    (SELECT count(*) FROM unnest($2::text[]) p WHERE c.content ILIKE p)::float8 AS score \
             FROM ms_rag_chunk c JOIN ms_rag_document d ON d.id = c.document_id \
             WHERE c.project_id = $1 AND c.content ILIKE ANY($2::text[]) AND {} \
             ORDER BY score DESC LIMIT $3",
            audience_where(4, 5, 6)
        ))
        .bind(project_id)
        .bind(&terms)
        .bind(top_k as i64)
        .bind(audience.is_admin)
        .bind(&audience.user_id)
        .bind(&audience.visible_group_ids)
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

#[cfg(test)]
mod provider_tests {
    use super::*;
    #[test]
    fn ollama_disables_thinking_and_uses_native_budget() {
        let v = chat_request("http://ollama:11434/api/chat", "qwen3:8b", 123, "s", "u", true);
        assert_eq!(v["think"], false);
        assert_eq!(v["options"]["num_predict"], 123);
        assert_eq!(v["stream"], true);
        assert!(v.get("max_tokens").is_none());
    }
    #[tokio::test]
    async fn native_stream_preserves_split_utf8_and_returns_only_answer() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let handle = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept");
            socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).expect("timeout");
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).expect("headers");
                request.push(byte[0]);
            }
            let headers = String::from_utf8(request).expect("headers utf8");
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|n| n.trim().parse::<usize>().ok())
                })
                .expect("length");
            let mut body = vec![0; length];
            socket.read_exact(&mut body).expect("body");
            let request: serde_json::Value = serde_json::from_slice(&body).expect("json");
            assert_eq!(request["think"], false);
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").expect("response");
            let text = "{\"message\":{\"thinking\":\"hidden\",\"content\":\"中文答案\"},\"done\":false}\n{\"done\":true}\n";
            let split = text.find('中').expect("Chinese") + 1;
            for part in [&text.as_bytes()[..split], &text.as_bytes()[split..]] {
                write!(socket, "{:x}\r\n", part.len()).expect("chunk size");
                socket.write_all(part).expect("chunk");
                socket.write_all(b"\r\n").expect("newline");
                socket.flush().expect("flush");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            socket.write_all(b"0\r\n\r\n").expect("end");
        });
        let cfg = crate::config::RagConfig::handle_from_env();
        {
            let mut c = cfg.write().expect("config");
            c.chat_url = format!("http://{address}/api/chat");
            c.chat_key.clear();
        }
        let chat = OpenAiChat::new(cfg);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let full = chat.complete_stream("system", "question", tx).await.expect("answer");
        assert_eq!(full, "中文答案");
        assert_eq!(rx.recv().await.as_deref(), Some("中文答案"));
        handle.join().expect("server");
    }

    #[test]
    fn openai_retains_compatible_payload() {
        let v =
            chat_request("https://example.com/v1/chat/completions", "model", 123, "s", "u", false);
        assert_eq!(v["max_tokens"], 123);
        assert!(v.get("think").is_none());
    }
}

#[cfg(test)]
mod embedding_limit_tests {
    use super::*;
    #[test]
    fn only_a_provider_context_bad_request_is_rechunkable() {
        let message = r#"{"error":{"message":"the input length exceeds the context length"}}"#;
        assert!(matches!(
            embedding_http_error(reqwest::StatusCode::BAD_REQUEST, message),
            RagError::EmbeddingInputTooLong
        ));
        assert!(matches!(
            embedding_http_error(reqwest::StatusCode::UNAUTHORIZED, message),
            RagError::Embedding(_)
        ));
        assert!(matches!(
            embedding_http_error(reqwest::StatusCode::BAD_REQUEST, "model not found"),
            RagError::Embedding(_)
        ));
    }
}
