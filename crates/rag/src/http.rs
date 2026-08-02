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
use crate::domain::{RagDocument, TraceStep};
use crate::ports::{Chat, Embedder, VectorStore};

#[derive(Clone)]
pub struct RagState {
    pub store: Arc<dyn VectorStore>,
    pub embedder: Arc<dyn Embedder>,
    pub chat: Arc<dyn Chat>,
    pub sessions: Arc<dyn SessionStore>,
}

impl FromRef<RagState> for Arc<dyn SessionStore> {
    fn from_ref(s: &RagState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    store: Arc<dyn VectorStore>,
    embedder: Arc<dyn Embedder>,
    chat: Arc<dyn Chat>,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/rag/document", post(ingest_document))
        .route("/rag/document/{id}", axum::routing::delete(delete_document))
        .route("/rag/ask/stream", post(ask_stream))
        .route("/rag/evaluate", post(evaluate_answer))
        .with_state(RagState { store, embedder, chat, sessions })
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
