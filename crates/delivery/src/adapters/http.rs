use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use crate::application::{DeliveryCmdError, DeliveryService};
use crate::domain::{AttemptStatus, DeliveryAttempt, ExecutionEvent, ExecutorKind};
use crate::ports::{TaskListFilter, TaskRow};

#[derive(Clone)]
struct DelState {
    svc: DeliveryService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<DelState> for Arc<dyn SessionStore> {
    fn from_ref(s: &DelState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(svc: DeliveryService, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/delivery", post(dispatch).get(list_by_task))
        // Static segment /delivery/tasks must be registered before /delivery/{id},
        // or the wildcard swallows it.
        .route("/delivery/tasks", get(list_tasks))
        .route("/delivery/collab-stats", get(collab_stats))
        .route("/delivery/{id}", get(get_attempt).delete(delete_attempt))
        .route("/delivery/{id}/running", post(report_running))
        .route("/delivery/{id}/complete", post(complete))
        .route("/delivery/{id}/fail", post(fail))
        .route("/delivery/{id}/stop", post(stop))
        .route("/delivery/{id}/events", post(record_event).get(list_events))
        .with_state(DelState { svc, sessions })
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct DispatchBody {
    decomposition_id: String,
    task_id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
    executor: String,
    #[serde(default)]
    context: Option<String>,
    #[serde(default)]
    instructions: Option<String>,
    /// Target a registered runtime by name; unset = any runtime with the capability.
    #[serde(default)]
    target_runtime: Option<String>,
}

#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    decomposition_id: String,
    task_id: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunningBody {
    run_id: String,
}

#[derive(Deserialize, ToSchema)]
struct CompleteBody {
    kind: String,
    reference: String,
    #[serde(default)]
    summary: String,
}

#[derive(Deserialize, ToSchema)]
struct FailBody {
    error: String,
}

#[derive(Deserialize, ToSchema)]
struct EventBody {
    kind: String,
    message: String,
    #[serde(default)]
    detail: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct EventResponse {
    seq: i64,
    kind: String,
    message: String,
    detail: Option<String>,
}

impl From<&ExecutionEvent> for EventResponse {
    fn from(e: &ExecutionEvent) -> Self {
        Self {
            seq: e.seq,
            kind: e.kind.as_str().to_string(),
            message: e.message.clone(),
            detail: e.detail.clone(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct DeliverableResponse {
    kind: String,
    reference: String,
    summary: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct AttemptResponse {
    id: String,
    decomposition_id: String,
    task_id: String,
    executor: String,
    target_runtime: Option<String>,
    status: String,
    run_id: Option<String>,
    deliverable: Option<DeliverableResponse>,
    error: Option<String>,
}

impl From<&DeliveryAttempt> for AttemptResponse {
    fn from(a: &DeliveryAttempt) -> Self {
        Self {
            id: a.id.clone(),
            decomposition_id: a.decomposition_id.clone(),
            task_id: a.task_id.clone(),
            executor: a.executor.as_str().to_string(),
            target_runtime: a.target_runtime.clone(),
            status: a.status.as_str().to_string(),
            run_id: a.run_id.clone(),
            deliverable: a.deliverable.as_ref().map(|d| DeliverableResponse {
                kind: d.kind.as_str().to_string(),
                reference: d.reference.clone(),
                summary: d.summary.clone(),
            }),
            error: a.error.clone(),
        }
    }
}

#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct TaskQuery {
    status: Option<String>,
    executor: Option<String>,
    #[serde(default)]
    active: bool,
    q: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct TaskItemResponse {
    id: String,
    decomposition_id: String,
    task_id: String,
    title: String,
    description: String,
    module: String,
    executor: String,
    target_runtime: Option<String>,
    status: String,
    result: String,
    completion_rate: i32,
    run_id: Option<String>,
    error: Option<String>,
    created_at: i64,
    event_count: i64,
}

impl From<&TaskRow> for TaskItemResponse {
    fn from(r: &TaskRow) -> Self {
        let a = &r.attempt;
        let (result, completion_rate) = match a.status {
            AttemptStatus::Dispatched => ("PENDING", 0),
            AttemptStatus::Running => ("PENDING", 50),
            AttemptStatus::Delivered => ("SUCCESS", 100),
            AttemptStatus::Failed => ("FAILED", 100),
            AttemptStatus::Stopped => ("STOPPED", 100),
        };
        let title =
            r.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| a.task_id.clone());
        let trimmed =
            |s: &Option<String>| s.clone().filter(|v| !v.trim().is_empty()).unwrap_or_default();
        Self {
            id: a.id.clone(),
            decomposition_id: a.decomposition_id.clone(),
            task_id: a.task_id.clone(),
            title,
            description: trimmed(&r.description),
            module: trimmed(&r.module),
            executor: a.executor.as_str().to_string(),
            target_runtime: a.target_runtime.clone(),
            status: a.status.as_str().to_string(),
            result: result.to_string(),
            completion_rate,
            run_id: a.run_id.clone(),
            error: a.error.clone(),
            created_at: r.created_at,
            event_count: r.event_count,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct TaskPageResponse {
    total: i64,
    current: i64,
    page_size: i64,
    total_pages: i64,
    items: Vec<TaskItemResponse>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct StopBody {
    #[serde(default)]
    reason: Option<String>,
}

fn cmd_err(e: DeliveryCmdError) -> Response {
    match e {
        DeliveryCmdError::NotFound => (StatusCode::NOT_FOUND, "attempt not found").into_response(),
        DeliveryCmdError::Validation(m) => (StatusCode::BAD_REQUEST, m).into_response(),
        DeliveryCmdError::Conflict(_) => {
            (StatusCode::CONFLICT, "attempt state conflict").into_response()
        }
        DeliveryCmdError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(post, path = "/delivery", tag = "delivery", request_body = DispatchBody, responses((status = 201, body = AttemptResponse)), security(("bearer" = [])))]
async fn dispatch(
    user: AuthUser,
    State(st): State<DelState>,
    Json(b): Json<DispatchBody>,
) -> Response {
    if !user.can("DELIVERY", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st
        .svc
        .dispatch(
            &b.decomposition_id,
            &b.task_id,
            &b.title,
            &b.description,
            &b.acceptance_criteria,
            &b.executor,
            b.context,
            b.instructions,
            b.target_runtime,
        )
        .await
    {
        Ok(a) => (StatusCode::CREATED, Json(AttemptResponse::from(&a))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(get, path = "/delivery", tag = "delivery", params(ListQuery), responses((status = 200, body = [AttemptResponse])), security(("bearer" = [])))]
async fn list_by_task(
    user: AuthUser,
    State(st): State<DelState>,
    Query(q): Query<ListQuery>,
) -> Response {
    if !user.can("DELIVERY", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.list_by_task(&q.decomposition_id, &q.task_id).await {
        Ok(list) => {
            let body: Vec<AttemptResponse> = list.iter().map(AttemptResponse::from).collect();
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(get, path = "/delivery/{id}", tag = "delivery", params(("id" = String, Path)), responses((status = 200, body = AttemptResponse), (status = 404)), security(("bearer" = [])))]
async fn get_attempt(
    user: AuthUser,
    State(st): State<DelState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("DELIVERY", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.get(&id).await {
        Ok(a) => (StatusCode::OK, Json(AttemptResponse::from(&a))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(post, path = "/delivery/{id}/running", tag = "delivery", params(("id" = String, Path)), request_body = RunningBody, responses((status = 200, body = AttemptResponse)), security(("bearer" = [])))]
async fn report_running(
    user: AuthUser,
    State(st): State<DelState>,
    Path(id): Path<String>,
    Json(b): Json<RunningBody>,
) -> Response {
    if !user.can("DELIVERY", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.report_running(&id, &b.run_id).await {
        Ok(a) => (StatusCode::OK, Json(AttemptResponse::from(&a))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(post, path = "/delivery/{id}/complete", tag = "delivery", params(("id" = String, Path)), request_body = CompleteBody, responses((status = 200, body = AttemptResponse)), security(("bearer" = [])))]
async fn complete(
    user: AuthUser,
    State(st): State<DelState>,
    Path(id): Path<String>,
    Json(b): Json<CompleteBody>,
) -> Response {
    if !user.can("DELIVERY", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.complete(&id, &b.kind, &b.reference, &b.summary).await {
        Ok(a) => (StatusCode::OK, Json(AttemptResponse::from(&a))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(post, path = "/delivery/{id}/fail", tag = "delivery", params(("id" = String, Path)), request_body = FailBody, responses((status = 200, body = AttemptResponse)), security(("bearer" = [])))]
async fn fail(
    user: AuthUser,
    State(st): State<DelState>,
    Path(id): Path<String>,
    Json(b): Json<FailBody>,
) -> Response {
    if !user.can("DELIVERY", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.fail(&id, &b.error).await {
        Ok(a) => (StatusCode::OK, Json(AttemptResponse::from(&a))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(post, path = "/delivery/{id}/events", tag = "delivery", params(("id" = String, Path)), request_body = EventBody, responses((status = 201, body = EventResponse)), security(("bearer" = [])))]
async fn record_event(
    user: AuthUser,
    State(st): State<DelState>,
    Path(id): Path<String>,
    Json(b): Json<EventBody>,
) -> Response {
    if !user.can("DELIVERY", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.record_event(&id, &b.kind, &b.message, b.detail.as_deref()).await {
        Ok(e) => (StatusCode::CREATED, Json(EventResponse::from(&e))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(get, path = "/delivery/{id}/events", tag = "delivery", params(("id" = String, Path)), responses((status = 200, body = [EventResponse])), security(("bearer" = [])))]
async fn list_events(
    user: AuthUser,
    State(st): State<DelState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("DELIVERY", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.events(&id).await {
        Ok(list) => {
            let body: Vec<EventResponse> = list.iter().map(EventResponse::from).collect();
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(
    get, path = "/delivery/tasks", tag = "delivery", params(TaskQuery),
    responses((status = 200, body = TaskPageResponse)), security(("bearer" = []))
)]
async fn list_tasks(
    user: AuthUser,
    State(st): State<DelState>,
    Query(q): Query<TaskQuery>,
) -> Response {
    if !user.can("DELIVERY", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let page = q.page.unwrap_or(1).max(1);
    let page_size = q.page_size.unwrap_or(20).clamp(1, 200);
    let filter = TaskListFilter {
        status: q.status.as_deref().and_then(AttemptStatus::parse),
        executor: q.executor.as_deref().and_then(ExecutorKind::parse),
        active_only: q.active,
        query: q.q,
        limit: page_size,
        offset: (page - 1) * page_size,
    };
    match st.svc.list_tasks(&filter).await {
        Ok(p) => {
            let total_pages = if p.total == 0 { 0 } else { (p.total + page_size - 1) / page_size };
            let body = TaskPageResponse {
                total: p.total,
                current: page,
                page_size,
                total_pages,
                items: p.items.iter().map(TaskItemResponse::from).collect(),
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => cmd_err(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct CollabQuery {
    project_id: String,
    /// When set, restrict stats to this requirement (requirement-detail view).
    #[serde(default)]
    requirement_id: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CollabRequirementItem {
    requirement_id: String,
    title: String,
    ai_tasks: i64,
    human_tasks: i64,
    ai_points: i64,
    human_points: i64,
    ai_attempts: i64,
    ai_delivered: i64,
    ai_failed: i64,
    ai_first_pass: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CollabDayItem {
    date: String,
    ai: i64,
    human: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CollabStatsResponse {
    items: Vec<CollabRequirementItem>,
    daily: Vec<CollabDayItem>,
}

// Human/AI collaboration stats: a task counts as AI when it is VERIFIED and has a
// DELIVERED delivery record, otherwise as human.
#[utoipa::path(
    get, path = "/delivery/collab-stats", tag = "delivery", params(CollabQuery),
    responses((status = 200, body = CollabStatsResponse), (status = 403)), security(("bearer" = []))
)]
async fn collab_stats(
    user: AuthUser,
    State(st): State<DelState>,
    Query(q): Query<CollabQuery>,
) -> Response {
    if !user.can("DELIVERY", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.collab_stats(&q.project_id, q.requirement_id.as_deref()).await {
        Ok(cs) => {
            let body = CollabStatsResponse {
                items: cs
                    .items
                    .into_iter()
                    .map(|r| CollabRequirementItem {
                        requirement_id: r.requirement_id,
                        title: r.title,
                        ai_tasks: r.ai_tasks,
                        human_tasks: r.human_tasks,
                        ai_points: r.ai_points,
                        human_points: r.human_points,
                        ai_attempts: r.ai_attempts,
                        ai_delivered: r.ai_delivered,
                        ai_failed: r.ai_failed,
                        ai_first_pass: r.ai_first_pass,
                    })
                    .collect(),
                daily: cs
                    .daily
                    .into_iter()
                    .map(|d| CollabDayItem { date: d.date, ai: d.ai, human: d.human })
                    .collect(),
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(
    post, path = "/delivery/{id}/stop", tag = "delivery", params(("id" = String, Path)),
    request_body = StopBody, responses((status = 200, body = AttemptResponse), (status = 404), (status = 409)),
    security(("bearer" = []))
)]
async fn stop(
    user: AuthUser,
    State(st): State<DelState>,
    Path(id): Path<String>,
    Json(b): Json<StopBody>,
) -> Response {
    if !user.can("DELIVERY", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.stop(&id, b.reason.as_deref().unwrap_or("")).await {
        Ok(a) => (StatusCode::OK, Json(AttemptResponse::from(&a))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(
    delete, path = "/delivery/{id}", tag = "delivery", params(("id" = String, Path)),
    responses((status = 204), (status = 404), (status = 409)), security(("bearer" = []))
)]
async fn delete_attempt(
    user: AuthUser,
    State(st): State<DelState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("DELIVERY", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.delete(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => cmd_err(e),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(dispatch, list_by_task, get_attempt, report_running, complete, fail, record_event, list_events, list_tasks, collab_stats, stop, delete_attempt),
    components(schemas(DispatchBody, RunningBody, CompleteBody, FailBody, EventBody, EventResponse, DeliverableResponse, AttemptResponse, TaskItemResponse, TaskPageResponse, CollabRequirementItem, CollabDayItem, CollabStatsResponse, StopBody)),
    tags((name = "delivery", description = "Delivery execution (AI executor)"))
)]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{InMemoryDeliveryRepository, StubAgentExecutor, StubBehavior};
    use crate::domain::{Deliverable, DeliverableKind};
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app_with(perms: &str, behavior: StubBehavior) -> (Router, String) {
        let svc = DeliveryService::new(
            Arc::new(InMemoryDeliveryRepository::new()),
            Arc::new(StubAgentExecutor::new(behavior)),
        );
        let sessions = Arc::new(InMemorySessionStore::new());
        let set = PermissionSet::from_raw([perms.to_string()]).expect("perms");
        let token = sessions.create("u", set, 3600).await.expect("token");
        (router(svc, sessions), token)
    }

    fn req(method: &str, uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b =
            Request::builder().method(method).uri(uri).header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    async fn json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    #[tokio::test]
    async fn async_dispatch_then_callback_complete() {
        let (app, t) = app_with(
            "DELIVERY:READ+EXECUTE+UPDATE",
            StubBehavior::Accept { run_id: "run-3".into() },
        )
        .await;
        let r = app
            .clone()
            .oneshot(req("POST", "/delivery", r#"{"decompositionId":"d1","taskId":"t1","title":"build","executor":"CLAUDE_CODE"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = json(r).await;
        assert_eq!(v["status"], "RUNNING");
        assert_eq!(v["runId"], "run-3");
        let id = v["id"].as_str().expect("id").to_string();

        let c = app
            .clone()
            .oneshot(req(
                "POST",
                &format!("/delivery/{id}/complete"),
                r#"{"kind":"PULL_REQUEST","reference":"pr/7","summary":"ok"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(c.status(), StatusCode::OK);
        assert_eq!(json(c).await["status"], "DELIVERED");

        let l = app
            .oneshot(req("GET", "/delivery?decompositionId=d1&taskId=t1", "", Some(&t)))
            .await
            .expect("r");
        assert_eq!(l.status(), StatusCode::OK);
        assert_eq!(json(l).await.as_array().expect("arr").len(), 1);
    }

    #[tokio::test]
    async fn sync_dispatch_delivers_immediately() {
        let deliverable = Deliverable {
            kind: DeliverableKind::Diff,
            reference: "branch:x".into(),
            summary: "done".into(),
        };
        let (app, t) =
            app_with("DELIVERY:READ+EXECUTE+UPDATE", StubBehavior::Complete { deliverable }).await;
        let r = app
            .oneshot(req(
                "POST",
                "/delivery",
                r#"{"decompositionId":"d1","taskId":"t1","title":"build","executor":"CODEX"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = json(r).await;
        assert_eq!(v["status"], "DELIVERED");
        assert_eq!(v["deliverable"]["reference"], "branch:x");
    }

    #[tokio::test]
    async fn rbac_dispatch_requires_execute() {
        let (app, _t) =
            app_with("DELIVERY:READ", StubBehavior::Accept { run_id: "r".into() }).await;
        assert_eq!(
            app.oneshot(req(
                "POST",
                "/delivery",
                r#"{"decompositionId":"d1","taskId":"t1","title":"x","executor":"CODEX"}"#,
                None
            ))
            .await
            .expect("r")
            .status(),
            StatusCode::UNAUTHORIZED
        );
        let (app, t) = app_with("DELIVERY:READ", StubBehavior::Accept { run_id: "r".into() }).await;
        assert_eq!(
            app.oneshot(req(
                "POST",
                "/delivery",
                r#"{"decompositionId":"d1","taskId":"t1","title":"x","executor":"CODEX"}"#,
                Some(&t)
            ))
            .await
            .expect("r")
            .status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn record_and_list_execution_events() {
        let (app, t) =
            app_with("DELIVERY:READ+EXECUTE+UPDATE", StubBehavior::Accept { run_id: "r".into() })
                .await;
        let r = app
            .clone()
            .oneshot(req(
                "POST",
                "/delivery",
                r#"{"decompositionId":"d1","taskId":"t1","title":"x","executor":"CLAUDE_CODE"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        let id = json(r).await["id"].as_str().expect("id").to_string();

        let e = app
            .clone()
            .oneshot(req(
                "POST",
                &format!("/delivery/{id}/events"),
                r#"{"kind":"DECISION","message":"uses argon2","detail":"PHC"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(e.status(), StatusCode::CREATED);
        assert_eq!(json(e).await["kind"], "DECISION");
        app.clone()
            .oneshot(req(
                "POST",
                &format!("/delivery/{id}/events"),
                r#"{"kind":"FILE_CHANGE","message":"edit auth.rs"}"#,
                Some(&t),
            ))
            .await
            .expect("r");

        let list = app
            .clone()
            .oneshot(req("GET", &format!("/delivery/{id}/events"), "", Some(&t)))
            .await
            .expect("r");
        assert_eq!(list.status(), StatusCode::OK);
        let arr = json(list).await;
        assert_eq!(arr.as_array().expect("a").len(), 2);
        assert_eq!(arr[0]["message"], "uses argon2");

        assert_eq!(
            app.oneshot(req(
                "POST",
                &format!("/delivery/{id}/events"),
                r#"{"kind":"X","message":"m"}"#,
                Some(&t)
            ))
            .await
            .expect("r")
            .status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn record_event_requires_update_permission() {
        let (app, t) = app_with("DELIVERY:READ", StubBehavior::Accept { run_id: "r".into() }).await;
        assert_eq!(
            app.oneshot(req(
                "POST",
                "/delivery/whatever/events",
                r#"{"kind":"LOG","message":"m"}"#,
                Some(&t)
            ))
            .await
            .expect("r")
            .status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn unknown_executor_400() {
        let (app, t) =
            app_with("DELIVERY:READ+EXECUTE", StubBehavior::Accept { run_id: "r".into() }).await;
        assert_eq!(
            app.oneshot(req(
                "POST",
                "/delivery",
                r#"{"decompositionId":"d1","taskId":"t1","title":"x","executor":"GPT"}"#,
                Some(&t)
            ))
            .await
            .expect("r")
            .status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn task_center_list_stop_and_delete_flow() {
        let (app, t) =
            app_with("DELIVERY:READ+EXECUTE+UPDATE", StubBehavior::Accept { run_id: "r".into() })
                .await;
        for tid in ["t1", "t2"] {
            app.clone()
                .oneshot(req("POST", "/delivery", &format!(r#"{{"decompositionId":"d1","taskId":"{tid}","title":"build {tid}","executor":"CODEX"}}"#), Some(&t)))
                .await
                .expect("r");
        }

        let l = app
            .clone()
            .oneshot(req("GET", "/delivery/tasks?page=1&pageSize=10", "", Some(&t)))
            .await
            .expect("r");
        assert_eq!(l.status(), StatusCode::OK);
        let v = json(l).await;
        assert_eq!(v["total"], 2);
        assert_eq!(v["items"].as_array().expect("arr").len(), 2);
        let item = &v["items"][0];
        assert_eq!(item["status"], "RUNNING");
        assert_eq!(item["result"], "PENDING");
        assert_eq!(item["completionRate"], 50);
        let id = item["id"].as_str().expect("id").to_string();

        let act = app
            .clone()
            .oneshot(req("GET", "/delivery/tasks?active=true", "", Some(&t)))
            .await
            .expect("r");
        assert_eq!(json(act).await["total"], 2);

        let stop = app
            .clone()
            .oneshot(req(
                "POST",
                &format!("/delivery/{id}/stop"),
                r#"{"reason":"manual stop"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(stop.status(), StatusCode::OK);
        assert_eq!(json(stop).await["status"], "STOPPED");

        assert_eq!(
            json(
                app.clone()
                    .oneshot(req("GET", "/delivery/tasks?active=true", "", Some(&t)))
                    .await
                    .expect("r")
            )
            .await["total"],
            1
        );

        let del = app
            .clone()
            .oneshot(req("DELETE", &format!("/delivery/{id}"), "", Some(&t)))
            .await
            .expect("r");
        assert_eq!(del.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            json(
                app.clone().oneshot(req("GET", "/delivery/tasks", "", Some(&t))).await.expect("r")
            )
            .await["total"],
            1
        );
    }

    #[tokio::test]
    async fn delete_running_conflicts_and_rbac() {
        let (app, t) =
            app_with("DELIVERY:READ+EXECUTE+UPDATE", StubBehavior::Accept { run_id: "r".into() })
                .await;
        let r = app
            .clone()
            .oneshot(req(
                "POST",
                "/delivery",
                r#"{"decompositionId":"d1","taskId":"t1","title":"x","executor":"CODEX"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        let id = json(r).await["id"].as_str().expect("id").to_string();
        assert_eq!(
            app.clone()
                .oneshot(req("DELETE", &format!("/delivery/{id}"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::CONFLICT
        );
        let (app2, ro) =
            app_with("DELIVERY:READ", StubBehavior::Accept { run_id: "r".into() }).await;
        assert_eq!(
            app2.oneshot(req("POST", "/delivery/whatever/stop", r#"{}"#, Some(&ro)))
                .await
                .expect("r")
                .status(),
            StatusCode::FORBIDDEN
        );
    }
}
