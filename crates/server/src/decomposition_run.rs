use std::collections::HashSet;
use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use delivery::application::DeliveryService;
use requirement::application::RequirementService;
use task::application::{TaskCmdError, TaskService};
use task::domain::TaskStatus;

#[derive(Clone)]
struct RunState {
    tasks: TaskService,
    delivery: DeliveryService,
    requirements: RequirementService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<RunState> for Arc<dyn SessionStore> {
    fn from_ref(s: &RunState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    tasks: TaskService,
    delivery: DeliveryService,
    requirements: RequirementService,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/decomposition/{id}/run", post(run_decomposition))
        .route("/decomposition/{id}/graph", get(graph_handler))
        .route("/decomposition/{id}/summary", get(summary_handler))
        .route("/decomposition/{id}/reassign", post(reassign_handler))
        .with_state(RunState { tasks, delivery, requirements, sessions })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunBody {
    #[serde(default = "default_executor")]
    executor: String,
    #[serde(default = "default_concurrency")]
    max_concurrency: usize,
}

fn default_executor() -> String {
    "CLAUDE_CODE".to_string()
}
fn default_concurrency() -> usize {
    4
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunResponse {
    decomposition_id: String,
    total: usize,
    verified: usize,
    failed: usize,
    blocked: usize,
    rounds: u32,
}

#[utoipa::path(
    post, path = "/decomposition/{id}/run", tag = "task",
    params(("id" = String, Path)),
    request_body = RunBody,
    responses((status = 200, body = RunResponse), (status = 403), (status = 404)),
    security(("bearer" = []))
)]
async fn run_decomposition(
    user: AuthUser,
    State(st): State<RunState>,
    Path(id): Path<String>,
    Json(body): Json<RunBody>,
) -> Response {
    if !user.can("TASK", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let dec = match st.tasks.get(&id).await {
        Ok(d) => d,
        Err(TaskCmdError::DecompositionNotFound) => {
            return (StatusCode::NOT_FOUND, "decomposition not found").into_response()
        }
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    };
    let total = dec.tasks.len();
    let max = body.max_concurrency.max(1);
    let sem = Arc::new(Semaphore::new(max));
    let executor = body.executor;

    let mut rounds = 0u32;
    // Cap = task count + 1, guarding against a no-progress infinite loop.
    let guard = total as u32 + 1;
    loop {
        let dec = match st.tasks.get(&id).await {
            Ok(d) => d,
            Err(_) => break,
        };
        let verified: HashSet<&str> = dec
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Verified)
            .map(|t| t.id.as_str())
            .collect();
        let ready: Vec<_> = dec
            .tasks
            .iter()
            .filter(|t| {
                t.status == TaskStatus::Pending
                    && t.dependencies.iter().all(|d| verified.contains(d.as_str()))
            })
            .cloned()
            .collect();
        if ready.is_empty() || rounds >= guard {
            break;
        }
        rounds += 1;

        let mut set: JoinSet<()> = JoinSet::new();
        for t in ready {
            let delivery = st.delivery.clone();
            let sem = sem.clone();
            let did = id.clone();
            let exec = executor.clone();
            set.spawn(async move {
                let _permit = sem.acquire_owned().await;
                let _ = delivery
                    .dispatch(
                        &did,
                        &t.id,
                        &t.title,
                        &t.description,
                        &t.acceptance_criteria,
                        &exec,
                        None,
                        None,
                        None,
                    )
                    .await;
            });
        }
        while set.join_next().await.is_some() {}
    }

    let final_dec = st.tasks.get(&id).await.unwrap_or(dec);
    let verified = final_dec.tasks.iter().filter(|t| t.status == TaskStatus::Verified).count();
    let failed = final_dec.tasks.iter().filter(|t| t.status == TaskStatus::Failed).count();
    let blocked = total - verified - failed;

    if total > 0 && verified == total {
        if let Err(e) = st.requirements.deliver(&final_dec.requirement_id, "orchestrator").await {
            tracing::warn!(requirement = %final_dec.requirement_id, "自动标记交付失败(可能未定基线): {e:?}");
        } else {
            tracing::info!(requirement = %final_dec.requirement_id, "需求已自动标记交付(DELIVERED)");
        }
    }

    (
        StatusCode::OK,
        Json(RunResponse { decomposition_id: id, total, verified, failed, blocked, rounds }),
    )
        .into_response()
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct GraphNodeDto {
    id: String,
    title: String,
    status: String,
    assignee: String,
    points: i32,
    layer: u32,
    ready: bool,
}

#[derive(Debug, Serialize, ToSchema)]
struct GraphEdgeDto {
    from: String,
    to: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct GraphResponse {
    decomposition_id: String,
    layers: u32,
    nodes: Vec<GraphNodeDto>,
    edges: Vec<GraphEdgeDto>,
}

/// Dependency-graph visualization data: nodes (with topological layer and readiness)
/// plus directed edges (dependency → dependent task).
#[utoipa::path(
    get, path = "/decomposition/{id}/graph", tag = "task",
    params(("id" = String, Path)),
    responses((status = 200, body = GraphResponse), (status = 403), (status = 404)),
    security(("bearer" = []))
)]
async fn graph_handler(
    user: AuthUser,
    State(st): State<RunState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("TASK", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let dec = match st.tasks.get(&id).await {
        Ok(d) => d,
        Err(TaskCmdError::DecompositionNotFound) => {
            return (StatusCode::NOT_FOUND, "decomposition not found").into_response()
        }
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    };
    let g = dec.graph_view();
    let nodes = g
        .nodes
        .into_iter()
        .map(|n| GraphNodeDto {
            id: n.id,
            title: n.title,
            status: n.status.to_string(),
            assignee: n.assignee,
            points: n.points,
            layer: n.layer,
            ready: n.ready,
        })
        .collect();
    let edges = g.edges.into_iter().map(|e| GraphEdgeDto { from: e.from, to: e.to }).collect();
    (StatusCode::OK, Json(GraphResponse { decomposition_id: id, layers: g.layers, nodes, edges }))
        .into_response()
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct SummaryResponse {
    decomposition_id: String,
    total: u64,
    pending: u64,
    dispatched: u64,
    running: u64,
    delivered: u64,
    verified: u64,
    failed: u64,
}

/// Task aggregation by status (decomposition dashboard): total plus per-status counts.
#[utoipa::path(
    get, path = "/decomposition/{id}/summary", tag = "task",
    params(("id" = String, Path)),
    responses((status = 200, body = SummaryResponse), (status = 403), (status = 404)),
    security(("bearer" = []))
)]
async fn summary_handler(
    user: AuthUser,
    State(st): State<RunState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("TASK", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let dec = match st.tasks.get(&id).await {
        Ok(d) => d,
        Err(TaskCmdError::DecompositionNotFound) => {
            return (StatusCode::NOT_FOUND, "decomposition not found").into_response()
        }
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    };
    let c = dec.status_summary();
    (
        StatusCode::OK,
        Json(SummaryResponse {
            decomposition_id: id,
            total: c.total(),
            pending: c.pending,
            dispatched: c.dispatched,
            running: c.running,
            delivered: c.delivered,
            verified: c.verified,
            failed: c.failed,
        }),
    )
        .into_response()
}

#[derive(Debug, Deserialize, ToSchema)]
struct ReassignBody {
    /// Current assignee (empty string matches unassigned tasks).
    from: String,
    /// New assignee (empty string clears the assignment).
    to: String,
    /// Assignee kind, e.g. AGENT / USER; ignored when `to` is empty.
    #[serde(default = "default_kind")]
    kind: String,
}

fn default_kind() -> String {
    "AGENT".to_string()
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReassignResponse {
    decomposition_id: String,
    reassigned: usize,
}

/// Bulk reassign: move one executor's unfinished tasks to another (skipping
/// completed ones); returns the number changed.
#[utoipa::path(
    post, path = "/decomposition/{id}/reassign", tag = "task",
    params(("id" = String, Path)),
    request_body = ReassignBody,
    responses((status = 200, body = ReassignResponse), (status = 403), (status = 404)),
    security(("bearer" = []))
)]
async fn reassign_handler(
    user: AuthUser,
    State(st): State<RunState>,
    Path(id): Path<String>,
    Json(body): Json<ReassignBody>,
) -> Response {
    if !user.can("TASK", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.tasks.reassign(&id, &body.from, &body.to, &body.kind).await {
        Ok((_, reassigned)) => {
            (StatusCode::OK, Json(ReassignResponse { decomposition_id: id, reassigned }))
                .into_response()
        }
        Err(TaskCmdError::DecompositionNotFound) => {
            (StatusCode::NOT_FOUND, "decomposition not found").into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(run_decomposition, graph_handler, summary_handler, reassign_handler),
    components(schemas(
        RunBody, RunResponse, GraphResponse, GraphNodeDto, GraphEdgeDto, SummaryResponse,
        ReassignBody, ReassignResponse
    )),
    tags((name = "task", description = "任务编排"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
