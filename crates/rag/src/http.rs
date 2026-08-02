//! HTTP adapter (feature `http`): document ingest + the streaming Q&A endpoint whose SSE events
//! (`sources` / `chunk` / `trace` / `done`) drive the Q&A UI and its decision-chain view.

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use webauth::{AuthUser, SessionStore};

use crate::application::{build_prompt, elapsed_since, ingest, now_ms, retrieve};
use crate::config::RagConfigHandle;
use crate::domain::{RagDocument, TraceStep};
use crate::ports::{Chat, Embedder, VectorStore};

#[derive(Clone)]
pub struct RagState {
    pub store: Arc<dyn VectorStore>,
    pub embedder: Arc<dyn Embedder>,
    pub chat: Arc<dyn Chat>,
    pub sessions: Arc<dyn SessionStore>,
    pub config: RagConfigHandle,
    pub pool: sqlx::PgPool,
}

impl FromRef<RagState> for Arc<dyn SessionStore> {
    fn from_ref(s: &RagState) -> Self {
        s.sessions.clone()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn router(
    store: Arc<dyn VectorStore>,
    embedder: Arc<dyn Embedder>,
    chat: Arc<dyn Chat>,
    sessions: Arc<dyn SessionStore>,
    config: RagConfigHandle,
    pool: sqlx::PgPool,
) -> Router {
    Router::new()
        .route("/rag/document", post(ingest_document))
        .route("/rag/document/{id}", axum::routing::delete(delete_document))
        .route("/rag/ask/stream", post(ask_stream))
        .route("/rag/evaluate", post(evaluate_answer))
        .route("/rag/review", post(review_requirement))
        .route("/system/rag/config", axum::routing::get(get_config).put(put_config))
        .with_state(RagState { store, embedder, chat, sessions, config, pool })
}

/// Overlay the persisted config row (if any) onto `handle`, keeping env values where a field is blank.
pub async fn load_config(pool: &sqlx::PgPool, handle: &RagConfigHandle) -> Result<(), String> {
    let row =
        sqlx::query_as::<_, (String, String, i32, String, String, String, String, i32, i32, bool)>(
            "SELECT embed_url, embed_model, embed_dim, embed_key, chat_url, chat_model, chat_key, \
                max_tokens, top_k, rerank FROM ms_rag_config WHERE id = 1",
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    if let Some((eu, em, ed, ek, cu, cm, ck, mt, tk, rr)) = row {
        let mut c = handle.write().map_err(|_| "config poisoned".to_string())?;
        if !eu.is_empty() {
            c.embed_url = eu;
        }
        if !em.is_empty() {
            c.embed_model = em;
        }
        if ed > 0 {
            c.embed_dim = ed as usize;
        }
        if !ek.is_empty() {
            c.embed_key = ek;
        }
        if !cu.is_empty() {
            c.chat_url = cu;
        }
        if !cm.is_empty() {
            c.chat_model = cm;
        }
        if !ck.is_empty() {
            c.chat_key = ck;
        }
        if mt > 0 {
            c.max_tokens = mt as u32;
        }
        if tk > 0 {
            c.top_k = tk as usize;
        }
        c.rerank = rr;
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigView {
    embed_url: String,
    embed_model: String,
    embed_dim: usize,
    embed_key_set: bool,
    chat_url: String,
    chat_model: String,
    chat_key_set: bool,
    max_tokens: u32,
    top_k: usize,
    rerank: bool,
}

async fn get_config(user: AuthUser, State(st): State<RagState>) -> Response {
    if !user.can("SYSTEM_USER", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let c = st.config.read().expect("rag config poisoned").clone();
    (
        StatusCode::OK,
        Json(ConfigView {
            embed_url: c.embed_url,
            embed_model: c.embed_model,
            embed_dim: c.embed_dim,
            embed_key_set: !c.embed_key.is_empty(),
            chat_url: c.chat_url,
            chat_model: c.chat_model,
            chat_key_set: !c.chat_key.is_empty(),
            max_tokens: c.max_tokens,
            top_k: c.top_k,
            rerank: c.rerank,
        }),
    )
        .into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigBody {
    embed_url: String,
    embed_model: String,
    embed_dim: usize,
    #[serde(default)]
    embed_key: Option<String>,
    chat_url: String,
    chat_model: String,
    #[serde(default)]
    chat_key: Option<String>,
    max_tokens: u32,
    top_k: usize,
    rerank: bool,
}

async fn put_config(
    user: AuthUser,
    State(st): State<RagState>,
    Json(b): Json<ConfigBody>,
) -> Response {
    if !user.can("SYSTEM_USER", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    // Apply to the live handle (blank key = keep existing), then persist the same values.
    let (embed_key, chat_key) = {
        let mut c = st.config.write().expect("rag config poisoned");
        c.embed_url = b.embed_url.clone();
        c.embed_model = b.embed_model.clone();
        c.embed_dim = b.embed_dim.max(1);
        c.chat_url = b.chat_url.clone();
        c.chat_model = b.chat_model.clone();
        c.max_tokens = b.max_tokens.max(1);
        c.top_k = b.top_k.clamp(1, 20);
        c.rerank = b.rerank;
        if let Some(k) = b.embed_key.as_ref().filter(|k| !k.is_empty()) {
            c.embed_key = k.clone();
        }
        if let Some(k) = b.chat_key.as_ref().filter(|k| !k.is_empty()) {
            c.chat_key = k.clone();
        }
        (c.embed_key.clone(), c.chat_key.clone())
    };
    let r = sqlx::query(
        "INSERT INTO ms_rag_config (id, embed_url, embed_model, embed_dim, embed_key, chat_url, \
             chat_model, chat_key, max_tokens, top_k, rerank, updated_at) \
         VALUES (1,$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,(extract(epoch from now())*1000)::bigint) \
         ON CONFLICT (id) DO UPDATE SET embed_url=$1, embed_model=$2, embed_dim=$3, embed_key=$4, \
             chat_url=$5, chat_model=$6, chat_key=$7, max_tokens=$8, top_k=$9, rerank=$10, \
             updated_at=(extract(epoch from now())*1000)::bigint",
    )
    .bind(&b.embed_url)
    .bind(&b.embed_model)
    .bind(b.embed_dim as i32)
    .bind(&embed_key)
    .bind(&b.chat_url)
    .bind(&b.chat_model)
    .bind(&chat_key)
    .bind(b.max_tokens as i32)
    .bind(b.top_k as i32)
    .bind(b.rerank)
    .execute(&st.pool)
    .await;
    match r {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IngestBody {
    project_id: String,
    title: String,
    text: String,
    #[serde(default)]
    source_type: Option<String>,
    #[serde(default)]
    source_id: Option<String>,
    /// Optional stable id (re-ingest replaces the same document); random uuid when absent.
    #[serde(default)]
    document_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestResponse {
    document_id: String,
    chunks: usize,
}

async fn ingest_document(
    _user: AuthUser,
    State(st): State<RagState>,
    Json(b): Json<IngestBody>,
) -> Response {
    if b.project_id.trim().is_empty() || b.text.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "projectId and text are required").into_response();
    }
    let ts = now_ms();
    let doc = RagDocument {
        id: b.document_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        project_id: b.project_id,
        source_type: b.source_type.unwrap_or_else(|| "manual".into()),
        source_id: b.source_id,
        title: b.title,
        created_at: ts,
        updated_at: ts,
    };
    let id = doc.id.clone();
    match ingest(st.store.as_ref(), st.embedder.as_ref(), doc, &b.text).await {
        Ok(chunks) => {
            (StatusCode::OK, Json(IngestResponse { document_id: id, chunks })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn delete_document(
    _user: AuthUser,
    State(st): State<RagState>,
    Path(id): Path<String>,
) -> Response {
    match st.store.delete_document(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AskBody {
    project_id: String,
    question: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
    #[serde(default)]
    trace: bool,
    #[serde(default = "default_true")]
    rerank: bool,
    /// Prior turns as [role, content] pairs (role = "user" | "assistant").
    #[serde(default)]
    history: Vec<(String, String)>,
    #[serde(default)]
    session_id: Option<String>,
}
fn default_top_k() -> usize {
    8
}
fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvalBody {
    project_id: String,
    question: String,
    answer: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
}

async fn evaluate_answer(
    _user: AuthUser,
    State(st): State<RagState>,
    Json(b): Json<EvalBody>,
) -> Response {
    match crate::application::evaluate(
        st.store.as_ref(),
        st.embedder.as_ref(),
        st.chat.as_ref(),
        &b.project_id,
        &b.question,
        &b.answer,
        b.top_k,
    )
    .await
    {
        Ok(e) => (StatusCode::OK, Json(e)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewBody {
    project_id: String,
    #[serde(default)]
    title: String,
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReviewResult {
    opinion: crate::domain::ReviewOpinion,
    sources: Vec<crate::domain::Hit>,
}

/// AI requirement-review opinion, grounded in the project's knowledge base. Advisory only — the
/// reviewer still approves/rejects via the requirement flow. Retrieval depth/rerank follow the
/// system RAG config.
async fn review_requirement(
    user: AuthUser,
    State(st): State<RagState>,
    Json(b): Json<ReviewBody>,
) -> Response {
    if !user.can("REQUIREMENT", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let (top_k, rerank) = {
        let c = st.config.read().expect("rag config poisoned");
        (c.top_k, c.rerank)
    };
    match crate::application::review(
        st.store.as_ref(),
        st.embedder.as_ref(),
        st.chat.as_ref(),
        &b.project_id,
        &b.title,
        &b.text,
        top_k,
        rerank,
    )
    .await
    {
        Ok((opinion, sources)) => {
            (StatusCode::OK, Json(ReviewResult { opinion, sources })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// SSE Q&A. Emits: `sources` (retrieved refs) → `chunk` (answer delta) → `trace` (decision chain,
/// when requested) → `done`. Errors surface as an `error` event so the stream always closes cleanly.
async fn ask_stream(
    _user: AuthUser,
    State(st): State<RagState>,
    Json(b): Json<AskBody>,
) -> Response {
    let RagState { store, embedder, chat, .. } = st;
    let session_id = b.session_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let stream = async_stream::stream! {
        let ev = |name: &str, data: serde_json::Value| -> std::result::Result<Event, std::convert::Infallible> {
            Ok(Event::default().event(name).json_data(data).unwrap_or_default())
        };
        let sid = session_id.clone();
        // 1) retrieve (embed → hybrid → fuse → rerank → context) and emit the sources up front
        match retrieve(&*store, &*embedder, &*chat, &b.project_id, &b.question, b.top_k, b.rerank).await {
            Err(e) => { yield ev("error", json!({ "message": e.to_string() })); }
            Ok((hits, context, mut trace)) => {
                let sources: Vec<_> = hits
                    .iter()
                    .map(|h| json!({
                        "doc_id": h.document_id,
                        "title": h.title,
                        "heading": h.heading,
                        "content_preview": h.content.chars().take(1500).collect::<String>(),
                        "relevance_score": h.score,
                    }))
                    .collect();
                yield ev("sources", json!({ "session_id": sid, "sources": sources }));

                // 2) stream the answer token-by-token: the LLM task sends deltas on a channel we forward as `chunk`s
                let (system, user) = build_prompt(&b.question, &context, &b.history);
                let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(64);
                let chat2 = chat.clone();
                let gen_start = std::time::Instant::now();
                let handle = tokio::spawn(async move { chat2.complete_stream(&system, &user, tx).await });
                let mut full = String::new();
                while let Some(delta) = rx.recv().await {
                    full.push_str(&delta);
                    yield ev("chunk", json!({ "delta": delta }));
                }
                // 3) finalize: add the generation step to the trace, then trace + done (or error)
                match handle.await {
                    Ok(Ok(_)) => {
                        if b.trace {
                            trace.steps.push(TraceStep::LlmGeneration {
                                latency_ms: gen_start.elapsed().as_millis() as u64,
                                answer_chars: full.chars().count(),
                            });
                            trace.total_ms = elapsed_since(&trace);
                            yield ev("trace", serde_json::to_value(&trace).unwrap_or_default());
                        }
                        yield ev("done", json!({ "session_id": sid }));
                    }
                    Ok(Err(e)) => { yield ev("error", json!({ "message": e.to_string() })); }
                    Err(e) => { yield ev("error", json!({ "message": format!("stream task failed: {e}") })); }
                }
            }
        }
    };
    Sse::new(stream).into_response()
}
