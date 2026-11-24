//! HTTP adapter (feature `http`): document ingest + the streaming Q&A endpoint whose SSE events
//! (`sources` / `chunk` / `trace` / `done`) drive the Q&A UI and its decision-chain view.

mod channels;
mod code;
mod impact;
mod onboarding;
mod sources;
#[cfg(test)]
mod test_support;

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use webauth::{AuthUser, SessionStore};

use crate::application::{elapsed_since, ingest, now_ms, retrieve};
use crate::config::RagConfigHandle;
use crate::domain::{Audience, RagDocument, RagError, TraceStep};
use crate::ports::{Chat, Embedder, VectorStore};

#[derive(Clone)]
pub struct RagState {
    pub store: Arc<dyn VectorStore>,
    pub embedder: Arc<dyn Embedder>,
    pub chat: Arc<dyn Chat>,
    pub sessions: Arc<dyn SessionStore>,
    pub config: RagConfigHandle,
    pub pool: sqlx::PgPool,
    pub changes: Option<Arc<dyn crate::ports::ChangeSource>>,
    pub queries: Option<Arc<dyn crate::grounding::QuerySource>>,
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
    router_with_changes(store, embedder, chat, sessions, config, pool, None)
}

#[allow(clippy::too_many_arguments)]
pub fn router_with_changes(
    store: Arc<dyn VectorStore>,
    embedder: Arc<dyn Embedder>,
    chat: Arc<dyn Chat>,
    sessions: Arc<dyn SessionStore>,
    config: RagConfigHandle,
    pool: sqlx::PgPool,
    changes: Option<Arc<dyn crate::ports::ChangeSource>>,
) -> Router {
    router_with_queries(store, embedder, chat, sessions, config, pool, changes, None)
}

#[allow(clippy::too_many_arguments)]
pub fn router_with_queries(
    store: Arc<dyn VectorStore>,
    embedder: Arc<dyn Embedder>,
    chat: Arc<dyn Chat>,
    sessions: Arc<dyn SessionStore>,
    config: RagConfigHandle,
    pool: sqlx::PgPool,
    changes: Option<Arc<dyn crate::ports::ChangeSource>>,
    queries: Option<Arc<dyn crate::grounding::QuerySource>>,
) -> Router {
    Router::new()
        .merge(code::routes())
        .merge(channels::routes())
        .merge(onboarding::routes())
        .merge(impact::routes())
        .route("/rag/document", post(ingest_document).get(list_documents))
        .route(
            "/rag/document/{id}",
            axum::routing::delete(delete_document).get(sources::get_document),
        )
        .route("/rag/document/{id}/audience", axum::routing::put(update_document_audience))
        .route("/rag/reindex", post(reindex_embeddings))
        .route("/rag/feedback", post(submit_feedback))
        .route("/rag/stats", axum::routing::get(kb_stats))
        .route("/rag/ask/stream", post(ask_stream))
        .route("/rag/evaluate", post(evaluate_answer))
        .route("/rag/review", post(review_requirement))
        .route("/rag/visibility-group", axum::routing::get(list_groups).post(create_group))
        .route("/rag/visibility-group/{id}", axum::routing::put(update_group).delete(delete_group))
        .route("/system/rag/config", axum::routing::get(get_config).put(put_config))
        .route("/system/rag/test", post(test_config))
        .with_state(RagState { store, embedder, chat, sessions, config, pool, changes, queries })
}

// Directory read permission is part of ordinary users' baseline; it must not bypass KB ACLs.
fn is_rag_admin(user: &AuthUser) -> bool {
    user.can("SYSTEM_USER", "UPDATE")
}

/// Project-scope gate: user-management administrators bypass membership; other users must belong
/// to the requested project. Directory read permission alone never bypasses membership.
async fn require_project_member(
    user: &AuthUser,
    pool: &sqlx::PgPool,
    project_id: &str,
) -> std::result::Result<(), Response> {
    if is_rag_admin(user) {
        return Ok(());
    }
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM ms_project_member WHERE project_id = $1 AND user_id = $2)",
    )
    .bind(project_id)
    .bind(&user.user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response())?;
    if !is_member {
        return Err(
            (StatusCode::FORBIDDEN, "permission denied: not a project member").into_response()
        );
    }
    Ok(())
}

/// Map a RagError to an HTTP response. A missing/blank provider config is a precondition-not-met, not
/// a server fault → 503 with the actionable message (so the UI can say "configure RAG in system settings"); genuine
/// failures stay 500.
fn rag_error_response(e: RagError) -> Response {
    match e {
        RagError::Config(m) => (StatusCode::SERVICE_UNAVAILABLE, m).into_response(),
        RagError::EmbeddingInputTooLong => {
            (StatusCode::BAD_REQUEST, "embedding input exceeds model context length")
                .into_response()
        }
        other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()).into_response(),
    }
}

/// Machine-readable code for an SSE `error` event so the Q&A UI can give a specific hint (e.g. a
/// "go configure" prompt for an unconfigured provider) instead of just echoing the raw message.
fn rag_error_code(e: &RagError) -> &'static str {
    match e {
        RagError::Config(_) => "not_configured",
        RagError::EmbeddingInputTooLong => "input_too_long",
        _ => "error",
    }
}

/// Build the retrieval audience for `user`: global admins (`SYSTEM_USER:READ`) see everything;
/// otherwise resolve the caller's roles (ms_role/ms_user_role) to the set of visibility groups whose
/// role_names overlap them — the groups the caller can see. Docs they uploaded stay visible via owner_id.
async fn load_audience(user: &AuthUser, pool: &sqlx::PgPool) -> Audience {
    let is_admin = is_rag_admin(user);
    let visible_group_ids: Vec<String> = if is_admin {
        Vec::new() // admin bypasses the group filter anyway; skip the lookup
    } else {
        sqlx::query_scalar(
            "SELECT g.id FROM rag_visibility_group g \
             WHERE g.role_names && ( \
                SELECT COALESCE(array_agg(r.name), ARRAY[]::text[]) FROM ms_user_role ur \
                JOIN ms_role r ON r.id = ur.role_id WHERE ur.user_id = $1)",
        )
        .bind(&user.user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    };
    Audience { user_id: Some(user.user_id.clone()), visible_group_ids, is_admin }
}

/// Authorize a project-scoped RAG operation: the caller must hold `RAG:<action>` (READ for Q&A,
/// ADD/DELETE for KB writes) and clear the [`require_project_member`] scope gate.
async fn authorize_project(
    user: &AuthUser,
    pool: &sqlx::PgPool,
    project_id: &str,
    action: &str,
) -> std::result::Result<(), Response> {
    if !user.can("RAG", action) {
        return Err(
            (StatusCode::FORBIDDEN, format!("permission denied: RAG:{action}")).into_response()
        );
    }
    require_project_member(user, pool, project_id).await
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Probe {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct TestResult {
    embed: Probe,
    chat: Probe,
}

/// Live connectivity check for the configured RAG providers: a tiny embed + chat round-trip, each
/// reported independently so the UI can show which side is misconfigured. Read-level system perm.
async fn test_config(user: AuthUser, State(st): State<RagState>) -> Response {
    if !user.can("SYSTEM_USER", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied: SYSTEM_USER:READ").into_response();
    }
    let t = std::time::Instant::now();
    let embed = match st.embedder.embed("ping").await {
        Ok(v) if !v.is_empty() => {
            Probe { ok: true, latency_ms: Some(t.elapsed().as_millis() as u64), error: None }
        }
        Ok(_) => {
            Probe { ok: false, latency_ms: None, error: Some("empty embedding vector".into()) }
        }
        Err(e) => Probe { ok: false, latency_ms: None, error: Some(e.to_string()) },
    };
    let t = std::time::Instant::now();
    let chat = match st.chat.complete("You are a connectivity probe.", "Reply with: OK").await {
        Ok(_) => Probe { ok: true, latency_ms: Some(t.elapsed().as_millis() as u64), error: None },
        Err(e) => Probe { ok: false, latency_ms: None, error: Some(e.to_string()) },
    };
    (StatusCode::OK, Json(TestResult { embed, chat })).into_response()
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
    /// Visibility-group ids this doc belongs to. Empty = restricted to the uploader + admins.
    #[serde(default)]
    visibility_groups: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestResponse {
    document_id: String,
    chunks: usize,
    /// False = stored keyword-only (no embedding provider configured); backfill via POST /rag/reindex.
    embedded: bool,
}

async fn ingest_document(
    user: AuthUser,
    State(st): State<RagState>,
    Json(b): Json<IngestBody>,
) -> Response {
    if b.project_id.trim().is_empty() || b.text.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "projectId and text are required").into_response();
    }
    if let Err(resp) = authorize_project(&user, &st.pool, &b.project_id, "ADD").await {
        return resp;
    }
    if b.source_type.as_deref() == Some("onboarding") {
        return (StatusCode::BAD_REQUEST, "publish maintained knowledge through /rag/onboarding")
            .into_response();
    }
    // Stable ids must not let an importer move/overwrite documents outside its scope.
    if let Some(id) = b.document_id.as_ref() {
        let existing = sqlx::query_as::<_, (String, Option<String>, String)>(
            "SELECT project_id, owner_id, source_type FROM ms_rag_document WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&st.pool)
        .await;
        match existing {
            Ok(Some((project, owner, kind)))
                if kind == "onboarding"
                    || project != b.project_id
                    || (!is_rag_admin(&user) && owner.as_deref() != Some(&user.user_id)) =>
            {
                return (StatusCode::FORBIDDEN, "document outside importer scope").into_response();
            }
            Err(e) => return rag_error_response(RagError::Backend(e.to_string())),
            _ => {}
        }
    }
    let ts = now_ms();
    let doc = RagDocument {
        id: b.document_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        project_id: b.project_id,
        source_type: b.source_type.unwrap_or_else(|| "manual".into()),
        source_id: b.source_id,
        title: b.title,
        owner_id: Some(user.user_id.clone()),
        visibility_groups: b.visibility_groups,
        created_at: ts,
        updated_at: ts,
    };
    let id = doc.id.clone();
    match ingest(st.store.as_ref(), st.embedder.as_ref(), doc, &b.text).await {
        Ok(o) => (
            StatusCode::OK,
            Json(IngestResponse { document_id: id, chunks: o.chunks, embedded: o.embedded }),
        )
            .into_response(),
        Err(e) => rag_error_response(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReindexBody {
    /// Backfill only this project's un-embedded chunks; omit to reindex the whole store (admin-only).
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReindexResponse {
    reindexed: usize,
}

/// Backfill embeddings for keyword-only chunks once a provider is configured. Per-project reindex
/// needs `RAG:UPDATE` + membership; a whole-store reindex (no projectId) is admin-only.
async fn reindex_embeddings(
    user: AuthUser,
    State(st): State<RagState>,
    Json(b): Json<ReindexBody>,
) -> Response {
    match b.project_id.as_deref() {
        Some(pid) => {
            if let Err(resp) = authorize_project(&user, &st.pool, pid, "UPDATE").await {
                return resp;
            }
        }
        None => {
            if !user.can("SYSTEM_USER", "UPDATE") {
                return (StatusCode::FORBIDDEN, "permission denied: SYSTEM_USER:UPDATE")
                    .into_response();
            }
        }
    }
    match crate::application::reindex(
        st.store.as_ref(),
        st.embedder.as_ref(),
        b.project_id.as_deref(),
    )
    .await
    {
        Ok(reindexed) => (StatusCode::OK, Json(ReindexResponse { reindexed })).into_response(),
        Err(e) => rag_error_response(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeedbackBody {
    project_id: String,
    #[serde(default)]
    session_id: Option<String>,
    /// "up" | "down".
    vote: String,
    #[serde(default)]
    question: String,
    #[serde(default)]
    answer: String,
    #[serde(default)]
    comment: String,
}

/// Record a thumbs up/down on an answer. Project-scoped like the Q&A itself (`RAG:READ` + membership).
async fn submit_feedback(
    user: AuthUser,
    State(st): State<RagState>,
    Json(b): Json<FeedbackBody>,
) -> Response {
    if let Err(resp) = authorize_project(&user, &st.pool, &b.project_id, "READ").await {
        return resp;
    }
    let vote: i16 = match b.vote.as_str() {
        "up" => 1,
        "down" => -1,
        _ => return (StatusCode::BAD_REQUEST, "vote must be 'up' or 'down'").into_response(),
    };
    let r = sqlx::query(
        "INSERT INTO ms_rag_feedback (project_id, session_id, user_id, vote, question, answer, comment, created_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(&b.project_id)
    .bind(&b.session_id)
    .bind(&user.user_id)
    .bind(vote)
    .bind(&b.question)
    .bind(&b.answer)
    .bind(&b.comment)
    .bind(now_ms())
    .execute(&st.pool)
    .await;
    match r {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatsView {
    documents: i64,
    chunks: i64,
}

/// Knowledge-base size for the landing card — counts only what the caller may retrieve (same audience
/// filter as search), so the number matches what they can actually ask about.
async fn kb_stats(
    user: AuthUser,
    State(st): State<RagState>,
    Query(q): Query<ListDocsQuery>,
) -> Response {
    if let Err(resp) = authorize_project(&user, &st.pool, &q.project_id, "READ").await {
        return resp;
    }
    let a = load_audience(&user, &st.pool).await;
    let vis =
        "($2 OR (d.owner_id IS NOT NULL AND d.owner_id = $3) OR d.visibility_groups && $4::text[])";
    let docs: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM ms_rag_document d WHERE d.project_id = $1 AND {vis}"
    ))
    .bind(&q.project_id)
    .bind(a.is_admin)
    .bind(&a.user_id)
    .bind(&a.visible_group_ids)
    .fetch_one(&st.pool)
    .await
    .unwrap_or(0);
    let chunks: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM ms_rag_chunk c JOIN ms_rag_document d ON d.id = c.document_id \
         WHERE c.project_id = $1 AND {vis}"
    ))
    .bind(&q.project_id)
    .bind(a.is_admin)
    .bind(&a.user_id)
    .bind(&a.visible_group_ids)
    .fetch_one(&st.pool)
    .await
    .unwrap_or(0);
    (StatusCode::OK, Json(StatsView { documents: docs, chunks })).into_response()
}

async fn delete_document(
    user: AuthUser,
    State(st): State<RagState>,
    Path(id): Path<String>,
) -> Response {
    // Resolve the doc's project first so scope is checked against the row, not a client-supplied id.
    let project_id: Option<String> =
        match sqlx::query_scalar("SELECT project_id FROM ms_rag_document WHERE id = $1")
            .bind(&id)
            .fetch_optional(&st.pool)
            .await
        {
            Ok(p) => p,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };
    let Some(project_id) = project_id else {
        return StatusCode::NO_CONTENT.into_response(); // already gone — idempotent
    };
    if let Err(resp) = authorize_project(&user, &st.pool, &project_id, "DELETE").await {
        return resp;
    }
    match st.store.delete_document(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListDocsQuery {
    project_id: String,
    page: Option<u32>,
    page_size: Option<u32>,
    search: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DocView {
    id: String,
    title: String,
    source_type: String,
    owner_id: Option<String>,
    visibility_groups: Vec<String>,
    updated_at: i64,
}

/// List a project's KB documents (for the management UI). Only docs the caller may see are returned —
/// the same audience filter as retrieval, so non-admins don't discover restricted titles.
async fn list_documents(
    user: AuthUser,
    State(st): State<RagState>,
    Query(q): Query<ListDocsQuery>,
) -> Response {
    if let Err(resp) = authorize_project(&user, &st.pool, &q.project_id, "READ").await {
        return resp;
    }
    let audience = load_audience(&user, &st.pool).await;
    let page = q.page.unwrap_or(1).max(1);
    let page_size = q.page_size.unwrap_or(20).clamp(1, 100);
    let search = format!("%{}%", q.search.as_deref().unwrap_or(""));
    let total = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM ms_rag_document d WHERE d.project_id=$1 \
         AND ($2 OR d.owner_id=$3 OR d.visibility_groups && $4::text[]) AND d.title ILIKE $5",
    )
    .bind(&q.project_id)
    .bind(audience.is_admin)
    .bind(&audience.user_id)
    .bind(&audience.visible_group_ids)
    .bind(&search)
    .fetch_one(&st.pool)
    .await;
    let total = match total {
        Ok(n) => n,
        Err(e) => return rag_error_response(RagError::Backend(e.to_string())),
    };
    let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Vec<String>, i64)>(
        "SELECT id, title, source_type, owner_id, visibility_groups, updated_at \
         FROM ms_rag_document d WHERE d.project_id = $1 \
           AND ($2 OR d.owner_id = $3 OR d.visibility_groups && $4::text[]) \
           AND d.title ILIKE $5 ORDER BY updated_at DESC, id LIMIT $6 OFFSET $7",
    )
    .bind(&q.project_id)
    .bind(audience.is_admin)
    .bind(&audience.user_id)
    .bind(&audience.visible_group_ids)
    .bind(&search)
    .bind(if q.page.is_some() { i64::from(page_size) } else { i64::MAX })
    .bind(i64::from(page - 1) * i64::from(page_size))
    .fetch_all(&st.pool)
    .await;
    match rows {
        Ok(rs) => {
            let items =
                rs.into_iter()
                    .map(|(id, title, source_type, owner_id, visibility_groups, updated_at)| {
                        DocView { id, title, source_type, owner_id, visibility_groups, updated_at }
                    })
                    .collect::<Vec<_>>();
            if q.page.is_some() {
                Json(serde_json::json!({"items": items, "total": total, "current": page, "pageSize": page_size})).into_response()
            } else {
                Json(items).into_response()
            }
        }
        Err(e) => rag_error_response(RagError::Backend(e.to_string())),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocAudienceBody {
    visibility_groups: Vec<String>,
}

/// Re-assign a document's visibility groups without re-ingesting it. Scope is checked against the
/// stored row's project; requires `RAG:UPDATE`.
async fn update_document_audience(
    user: AuthUser,
    State(st): State<RagState>,
    Path(id): Path<String>,
    Json(b): Json<DocAudienceBody>,
) -> Response {
    let project_id: Option<String> =
        match sqlx::query_scalar("SELECT project_id FROM ms_rag_document WHERE id = $1")
            .bind(&id)
            .fetch_optional(&st.pool)
            .await
        {
            Ok(p) => p,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };
    let Some(project_id) = project_id else {
        return (StatusCode::NOT_FOUND, "document not found").into_response();
    };
    if let Err(resp) = authorize_project(&user, &st.pool, &project_id, "UPDATE").await {
        return resp;
    }
    let r = sqlx::query(
        "UPDATE ms_rag_document SET visibility_groups = $1, updated_at = $2 WHERE id = $3",
    )
    .bind(&b.visibility_groups)
    .bind(now_ms())
    .bind(&id)
    .execute(&st.pool)
    .await;
    match r {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------- Visibility groups (admin-managed taxonomy) ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupView {
    id: String,
    name: String,
    role_names: Vec<String>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupBody {
    name: String,
    #[serde(default)]
    role_names: Vec<String>,
}

fn to_group_view(r: (String, String, Vec<String>, i64, i64)) -> GroupView {
    GroupView { id: r.0, name: r.1, role_names: r.2, created_at: r.3, updated_at: r.4 }
}

/// List visibility groups (any RAG reader — needed to pick groups when tagging a doc).
async fn list_groups(user: AuthUser, State(st): State<RagState>) -> Response {
    if !user.can("RAG", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied: RAG:READ").into_response();
    }
    let rows = sqlx::query_as::<_, (String, String, Vec<String>, i64, i64)>(
        "SELECT id, name, role_names, created_at, updated_at FROM rag_visibility_group ORDER BY name",
    )
    .fetch_all(&st.pool)
    .await;
    match rows {
        Ok(rs) => (StatusCode::OK, Json(rs.into_iter().map(to_group_view).collect::<Vec<_>>()))
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Create a visibility group. Managing the taxonomy is an admin action (`SYSTEM_USER:UPDATE`).
async fn create_group(
    user: AuthUser,
    State(st): State<RagState>,
    Json(b): Json<GroupBody>,
) -> Response {
    if !user.can("SYSTEM_USER", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied: SYSTEM_USER:UPDATE").into_response();
    }
    if b.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "name is required").into_response();
    }
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now_ms();
    let r = sqlx::query_as::<_, (String, String, Vec<String>, i64, i64)>(
        "INSERT INTO rag_visibility_group (id, name, role_names, created_at, updated_at) \
         VALUES ($1,$2,$3,$4,$4) RETURNING id, name, role_names, created_at, updated_at",
    )
    .bind(&id)
    .bind(b.name.trim())
    .bind(&b.role_names)
    .bind(ts)
    .fetch_one(&st.pool)
    .await;
    match r {
        Ok(row) => (StatusCode::CREATED, Json(to_group_view(row))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Rename a group or change its roles. Takes effect immediately for every doc in the group.
async fn update_group(
    user: AuthUser,
    State(st): State<RagState>,
    Path(id): Path<String>,
    Json(b): Json<GroupBody>,
) -> Response {
    if !user.can("SYSTEM_USER", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied: SYSTEM_USER:UPDATE").into_response();
    }
    if b.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "name is required").into_response();
    }
    let r = sqlx::query(
        "UPDATE rag_visibility_group SET name = $1, role_names = $2, updated_at = $3 WHERE id = $4",
    )
    .bind(b.name.trim())
    .bind(&b.role_names)
    .bind(now_ms())
    .bind(&id)
    .execute(&st.pool)
    .await;
    match r {
        Ok(res) if res.rows_affected() == 0 => {
            (StatusCode::NOT_FOUND, "group not found").into_response()
        }
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Delete a group and strip its id from any docs that referenced it (no dangling group ids).
async fn delete_group(
    user: AuthUser,
    State(st): State<RagState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("SYSTEM_USER", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied: SYSTEM_USER:UPDATE").into_response();
    }
    let mut tx = match st.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    if let Err(e) = sqlx::query(
        "UPDATE ms_rag_document SET visibility_groups = array_remove(visibility_groups, $1) \
         WHERE $1 = ANY(visibility_groups)",
    )
    .bind(&id)
    .execute(&mut *tx)
    .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    if let Err(e) = sqlx::query("DELETE FROM rag_visibility_group WHERE id = $1")
        .bind(&id)
        .execute(&mut *tx)
        .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    match tx.commit().await {
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
    user: AuthUser,
    State(st): State<RagState>,
    Json(b): Json<EvalBody>,
) -> Response {
    if let Err(resp) = authorize_project(&user, &st.pool, &b.project_id, "READ").await {
        return resp;
    }
    let audience = load_audience(&user, &st.pool).await;
    match crate::application::evaluate(
        st.store.as_ref(),
        st.embedder.as_ref(),
        st.chat.as_ref(),
        &b.project_id,
        &b.question,
        &b.answer,
        b.top_k,
        &audience,
    )
    .await
    {
        Ok(e) => (StatusCode::OK, Json(e)).into_response(),
        Err(e) => rag_error_response(e),
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
    if let Err(resp) = require_project_member(&user, &st.pool, &b.project_id).await {
        return resp;
    }
    let audience = load_audience(&user, &st.pool).await;
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
        &audience,
    )
    .await
    {
        Ok((opinion, sources)) => {
            (StatusCode::OK, Json(ReviewResult { opinion, sources })).into_response()
        }
        Err(e) => rag_error_response(e),
    }
}

/// SSE Q&A. Emits: `sources` (retrieved refs) → `chunk` (answer delta) → `trace` (decision chain,
/// when requested) → `done`. Errors surface as an `error` event so the stream always closes cleanly.
async fn ask_stream(
    user: AuthUser,
    headers: axum::http::HeaderMap,
    State(st): State<RagState>,
    Json(b): Json<AskBody>,
) -> Response {
    if let Err(resp) = authorize_project(&user, &st.pool, &b.project_id, "READ").await {
        return resp;
    }
    let audience = load_audience(&user, &st.pool).await;
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let query = crate::grounding::QueryContext {
        user: user.clone(),
        project_id: b.project_id.clone(),
        question: b.question.clone(),
        history: b.history.clone(),
        at_ms: now_ms(),
    };
    let RagState { store, embedder, chat, pool, queries, .. } = st;
    let session_id = b.session_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let stream = async_stream::stream! {
        let ev = |name: &str, data: serde_json::Value| -> std::result::Result<Event, std::convert::Infallible> {
            Ok(Event::default().event(name).json_data(data).unwrap_or_default())
        };
        let sid = session_id.clone();
        if let Some(ref provider) = queries {
            yield ev("status", json!({"stage":"routing","message":"正在选择数据来源"}));
            match provider.answer(&query, &authorization).await {
                Ok(Some(answer)) => {
                    yield ev("sources", json!({"session_id":sid,"sources":answer.sources}));
                    yield ev("scope", answer.scope);
                    for chart in answer.charts { yield ev("chart", chart); }
                    yield ev("chunk", json!({"delta":answer.content}));
                    yield ev("done", json!({"session_id":sid,"grounded":true}));
                    return;
                }
                Ok(None) => {}
                Err(error) => {
                    yield ev("scope", json!({"status":"failed","projectId":query.project_id}));
                    yield ev("chunk", json!({"delta":format!("本次业务数据查询未完成，不能据此判断没有记录。\n\n{error}")}));
                    yield ev("done", json!({"session_id":sid,"grounded":true,"status":"failed"}));
                    return;
                }
            }
        }
        // 1) retrieve (embed → hybrid → fuse → rerank → context) and emit the sources up front
        match retrieve(&*store, &*embedder, &*chat, &b.project_id, &b.question, b.top_k, b.rerank, &audience).await {
            Err(e) => { yield ev("error", json!({ "message": e.to_string(), "code": rag_error_code(&e) })); }
            Ok((mut hits, mut context, mut trace)) => {
                let mut sources = match crate::references::load(&pool, &b.project_id, &hits, &audience).await {
                    Ok(sources) => sources,
                    Err(error) => { yield ev("error", json!({"message":error.to_string(),"code":rag_error_code(&error)})); return; }
                };
                if crate::references::recency_requested(&b.question) {
                    let mut ordered: Vec<_> = hits.into_iter().zip(sources).collect();
                    ordered.sort_by_key(|(_, source)| std::cmp::Reverse(crate::references::date_key(source)));
                    (hits, sources) = ordered.into_iter().unzip();
                    trace.steps.retain(|step| !matches!(step, TraceStep::ContextBuilt { .. }));
                    trace.steps.push(TraceStep::Fusion { method: "event-date-within-retrieved-candidates", candidates: hits.len(), selected: hits.len() });
                    context = crate::application::build_context(&hits, &mut trace);
                }
                let mut source_values: Vec<serde_json::Value> = sources.iter().filter_map(|source|serde_json::to_value(source).ok()).collect();
                if let Some(ref provider)=queries {
                    source_values=provider.reference_links(&query,&authorization,source_values.clone()).await.unwrap_or(source_values);
                }
                yield ev("sources", json!({ "session_id": sid, "sources": source_values }));
                if hits.is_empty() && trace.steps.iter().any(|step|matches!(step,TraceStep::Fusion {method:"maintained-onboarding",..})) {
                    yield ev("scope",json!({"kind":"onboarding_knowledge","status":"missing_visible_material","projectId":b.project_id}));
                    yield ev("chunk",json!({"delta":"当前项目尚无你有权访问的已维护入职资料，无法核实具体办理手续或公司制度。请在“入职知识”中维护真实指南／FAQ，并配置可见组后再提问。"}));
                    yield ev("done",json!({"session_id":sid,"grounded":true}));
                    return;
                }
                if crate::grounding::needs_change_proof(&b.question) {
                    yield ev("scope", json!({"status":"evidence_required","coverage":"retrieved_candidates_only","candidateSources":source_values.len()}));
                    yield ev("chunk", json!({"delta":"当前检索结果是候选资料，尚未完成具体提交差异核验，不能据此确认是谁修改或升级了版本。\n\n需要核对：仓库与分支、提交 SHA、依赖版本变更前后值、实际作者与提交时间。生产故障还需要部署版本和报错日志证据。每日活动文档的作者不能直接作为升级责任人。\n\n本次没有执行全仓库或全部分支扫描；请先查看下方来源。"}));
                    yield ev("done", json!({"session_id":sid,"status":"evidence_required"}));
                    return;
                }


                // 2) stream the answer token-by-token: the LLM task sends deltas on a channel we forward as `chunk`s
                let (system, user) = crate::application::build_prompt_at(&b.question, &context, &b.history, trace.started_at.parse().unwrap_or_else(|_| crate::application::now_ms()));
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
                    Ok(Err(e)) => { yield ev("error", json!({ "message": e.to_string(), "code": rag_error_code(&e) })); }
                    Err(e) => { yield ev("error", json!({ "message": format!("stream task failed: {e}"), "code": "error" })); }
                }
            }
        }
    };
    Sse::new(stream).into_response()
}

#[cfg(test)]
mod audience_policy_tests {
    use super::*;
    #[test]
    fn directory_readers_do_not_bypass_project_or_document_scope() {
        let reader = AuthUser {
            user_id: "reader".into(),
            permissions: webauth::PermissionSet::from_raw(["SYSTEM_USER:READ", "RAG:READ"])
                .expect("permissions"),
        };
        assert!(!is_rag_admin(&reader));
        let admin = AuthUser {
            user_id: "admin".into(),
            permissions: webauth::PermissionSet::from_raw(["SYSTEM_USER:READ+UPDATE", "RAG:READ"])
                .expect("permissions"),
        };
        assert!(is_rag_admin(&admin));
    }
}
