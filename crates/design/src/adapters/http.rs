use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use webauth::{AuthUser, SessionStore};

use crate::application::{ProposalCmdError, ProposalService};
use crate::domain::Proposal;

#[derive(Clone)]
struct DesignState {
    svc: ProposalService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<DesignState> for Arc<dyn SessionStore> {
    fn from_ref(s: &DesignState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(svc: ProposalService, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/proposal", post(create).get(list_by_requirement))
        .route("/proposal/{id}", get(get_proposal))
        .route("/proposal/{id}/design", post(submit_design))
        .route("/proposal/{id}/approve", post(approve))
        .route("/proposal/{id}/request-changes", post(request_changes))
        .with_state(DesignState { svc, sessions })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProposalResponse {
    id: String,
    requirement_id: String,
    title: String,
    design_doc: Option<String>,
    status: String,
    review_comment: Option<String>,
    revision: u32,
}

impl From<&Proposal> for ProposalResponse {
    fn from(p: &Proposal) -> Self {
        Self {
            id: p.id.clone(),
            requirement_id: p.requirement_id.clone(),
            title: p.title.clone(),
            design_doc: p.design_doc.clone(),
            status: p.status.as_str().to_string(),
            review_comment: p.review_comment.clone(),
            revision: p.revision,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBody {
    requirement_id: String,
    title: String,
}

#[derive(Deserialize)]
struct DesignBody {
    doc: String,
}

#[derive(Deserialize)]
struct CommentBody {
    comment: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    requirement_id: String,
}

fn cmd_err(e: ProposalCmdError) -> Response {
    match e {
        ProposalCmdError::NotFound => (StatusCode::NOT_FOUND, "proposal not found").into_response(),
        ProposalCmdError::Validation(m) => (StatusCode::BAD_REQUEST, m).into_response(),
        ProposalCmdError::Conflict(_) => {
            (StatusCode::CONFLICT, "proposal state conflict").into_response()
        }
        ProposalCmdError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

fn ok(p: &Proposal, code: StatusCode) -> Response {
    (code, Json(ProposalResponse::from(p))).into_response()
}

async fn create(
    user: AuthUser,
    State(st): State<DesignState>,
    Json(b): Json<CreateBody>,
) -> Response {
    if !user.can("REQUIREMENT", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.create(&b.requirement_id, &b.title).await {
        Ok(p) => ok(&p, StatusCode::CREATED),
        Err(e) => cmd_err(e),
    }
}

async fn list_by_requirement(
    State(st): State<DesignState>,
    Query(q): Query<ListQuery>,
) -> Response {
    match st.svc.list_by_requirement(&q.requirement_id).await {
        Ok(list) => {
            let body: Vec<ProposalResponse> = list.iter().map(ProposalResponse::from).collect();
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => cmd_err(e),
    }
}

async fn get_proposal(State(st): State<DesignState>, Path(id): Path<String>) -> Response {
    match st.svc.get(&id).await {
        Ok(p) => ok(&p, StatusCode::OK),
        Err(e) => cmd_err(e),
    }
}

async fn submit_design(
    user: AuthUser,
    State(st): State<DesignState>,
    Path(id): Path<String>,
    Json(b): Json<DesignBody>,
) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.submit_design(&id, &b.doc).await {
        Ok(p) => ok(&p, StatusCode::OK),
        Err(e) => cmd_err(e),
    }
}

async fn approve(
    user: AuthUser,
    State(st): State<DesignState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.approve(&id).await {
        Ok(p) => ok(&p, StatusCode::OK),
        Err(e) => cmd_err(e),
    }
}

async fn request_changes(
    user: AuthUser,
    State(st): State<DesignState>,
    Path(id): Path<String>,
    Json(b): Json<CommentBody>,
) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.request_changes(&id, &b.comment).await {
        Ok(p) => ok(&p, StatusCode::OK),
        Err(e) => cmd_err(e),
    }
}
