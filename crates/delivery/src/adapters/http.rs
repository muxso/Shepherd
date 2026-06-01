//! 交付的 HTTP 适配器。DTO ↔ 用例,命令错误映射 HTTP 码。
//!
//! RBAC(资源键 `DELIVERY`):派发任务给执行者需 `DELIVERY:EXECUTE`;
//! 异步回调(开跑/交付/失败)需 `DELIVERY:UPDATE`;读端点开放。
//! 错误码:校验→400,非法流转→409,尝试不存在→404。

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

use crate::application::{DeliveryCmdError, DeliveryService};
use crate::domain::{DeliveryAttempt, ExecutionEvent};

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
        .route("/delivery/{id}", get(get_attempt))
        .route("/delivery/{id}/running", post(report_running))
        .route("/delivery/{id}/complete", post(complete))
        .route("/delivery/{id}/fail", post(fail))
        .route("/delivery/{id}/events", post(record_event).get(list_events))
        .with_state(DelState { svc, sessions })
}

// ---- DTO ----

#[derive(Deserialize)]
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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    decomposition_id: String,
    task_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunningBody {
    run_id: String,
}

#[derive(Deserialize)]
struct CompleteBody {
    kind: String,
    reference: String,
    #[serde(default)]
    summary: String,
}

#[derive(Deserialize)]
struct FailBody {
    error: String,
}

#[derive(Deserialize)]
struct EventBody {
    kind: String,
    message: String,
    #[serde(default)]
    detail: Option<String>,
}

#[derive(Serialize)]
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeliverableResponse {
    kind: String,
    reference: String,
    summary: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AttemptResponse {
    id: String,
    decomposition_id: String,
    task_id: String,
    executor: String,
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

fn cmd_err(e: DeliveryCmdError) -> Response {
    match e {
        DeliveryCmdError::NotFound => (StatusCode::NOT_FOUND, "attempt not found").into_response(),
        DeliveryCmdError::Validation(m) => (StatusCode::BAD_REQUEST, m).into_response(),
        DeliveryCmdError::Conflict(_) => (StatusCode::CONFLICT, "attempt state conflict").into_response(),
        DeliveryCmdError::Repo(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

// ---- 处理器 ----

async fn dispatch(user: AuthUser, State(st): State<DelState>, Json(b): Json<DispatchBody>) -> Response {
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
        )
        .await
    {
        Ok(a) => (StatusCode::CREATED, Json(AttemptResponse::from(&a))).into_response(),
        Err(e) => cmd_err(e),
    }
}

async fn list_by_task(State(st): State<DelState>, Query(q): Query<ListQuery>) -> Response {
    match st.svc.list_by_task(&q.decomposition_id, &q.task_id).await {
        Ok(list) => {
            let body: Vec<AttemptResponse> = list.iter().map(AttemptResponse::from).collect();
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => cmd_err(e),
    }
}

async fn get_attempt(State(st): State<DelState>, Path(id): Path<String>) -> Response {
    match st.svc.get(&id).await {
        Ok(a) => (StatusCode::OK, Json(AttemptResponse::from(&a))).into_response(),
        Err(e) => cmd_err(e),
    }
}

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

async fn list_events(State(st): State<DelState>, Path(id): Path<String>) -> Response {
    match st.svc.events(&id).await {
        Ok(list) => {
            let body: Vec<EventResponse> = list.iter().map(EventResponse::from).collect();
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => cmd_err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use crate::adapters::{InMemoryDeliveryRepository, StubAgentExecutor, StubBehavior};
    use crate::domain::{Deliverable, DeliverableKind};
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
        let mut b = Request::builder().method(method).uri(uri).header("content-type", "application/json");
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
        let (app, t) = app_with("DELIVERY:READ+EXECUTE+UPDATE", StubBehavior::Accept { run_id: "run-3".into() }).await;
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

        // 回调交付
        let c = app
            .clone()
            .oneshot(req("POST", &format!("/delivery/{id}/complete"), r#"{"kind":"PULL_REQUEST","reference":"pr/7","summary":"ok"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(c.status(), StatusCode::OK);
        assert_eq!(json(c).await["status"], "DELIVERED");

        // list by task
        let l = app.oneshot(req("GET", "/delivery?decompositionId=d1&taskId=t1", "", Some(&t))).await.expect("r");
        assert_eq!(l.status(), StatusCode::OK);
        assert_eq!(json(l).await.as_array().expect("arr").len(), 1);
    }

    #[tokio::test]
    async fn sync_dispatch_delivers_immediately() {
        let deliverable = Deliverable { kind: DeliverableKind::Diff, reference: "branch:x".into(), summary: "done".into() };
        let (app, t) = app_with("DELIVERY:READ+EXECUTE+UPDATE", StubBehavior::Complete { deliverable }).await;
        let r = app
            .oneshot(req("POST", "/delivery", r#"{"decompositionId":"d1","taskId":"t1","title":"build","executor":"CODEX"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = json(r).await;
        assert_eq!(v["status"], "DELIVERED");
        assert_eq!(v["deliverable"]["reference"], "branch:x");
    }

    #[tokio::test]
    async fn rbac_dispatch_requires_execute() {
        // 无令牌 → 401
        let (app, _t) = app_with("DELIVERY:READ", StubBehavior::Accept { run_id: "r".into() }).await;
        assert_eq!(
            app.oneshot(req("POST", "/delivery", r#"{"decompositionId":"d1","taskId":"t1","title":"x","executor":"CODEX"}"#, None)).await.expect("r").status(),
            StatusCode::UNAUTHORIZED
        );
        // 只读令牌 → 403
        let (app, t) = app_with("DELIVERY:READ", StubBehavior::Accept { run_id: "r".into() }).await;
        assert_eq!(
            app.oneshot(req("POST", "/delivery", r#"{"decompositionId":"d1","taskId":"t1","title":"x","executor":"CODEX"}"#, Some(&t))).await.expect("r").status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn record_and_list_execution_events() {
        let (app, t) = app_with("DELIVERY:READ+EXECUTE+UPDATE", StubBehavior::Accept { run_id: "r".into() }).await;
        let r = app
            .clone()
            .oneshot(req("POST", "/delivery", r#"{"decompositionId":"d1","taskId":"t1","title":"x","executor":"CLAUDE_CODE"}"#, Some(&t)))
            .await
            .expect("r");
        let id = json(r).await["id"].as_str().expect("id").to_string();

        // 上报执行事件
        let e = app
            .clone()
            .oneshot(req("POST", &format!("/delivery/{id}/events"), r#"{"kind":"DECISION","message":"选用 argon2","detail":"PHC"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(e.status(), StatusCode::CREATED);
        assert_eq!(json(e).await["kind"], "DECISION");
        app.clone().oneshot(req("POST", &format!("/delivery/{id}/events"), r#"{"kind":"FILE_CHANGE","message":"edit auth.rs"}"#, Some(&t))).await.expect("r");

        // 审计读取
        let list = app.clone().oneshot(req("GET", &format!("/delivery/{id}/events"), "", Some(&t))).await.expect("r");
        assert_eq!(list.status(), StatusCode::OK);
        let arr = json(list).await;
        assert_eq!(arr.as_array().expect("a").len(), 2);
        assert_eq!(arr[0]["message"], "选用 argon2");

        // 未知 kind → 400
        assert_eq!(
            app.oneshot(req("POST", &format!("/delivery/{id}/events"), r#"{"kind":"X","message":"m"}"#, Some(&t))).await.expect("r").status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn record_event_requires_update_permission() {
        let (app, t) = app_with("DELIVERY:READ", StubBehavior::Accept { run_id: "r".into() }).await;
        // 只读令牌不能上报事件 → 403(尝试不存在也会先过 RBAC)
        assert_eq!(
            app.oneshot(req("POST", "/delivery/whatever/events", r#"{"kind":"LOG","message":"m"}"#, Some(&t))).await.expect("r").status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn unknown_executor_400() {
        let (app, t) = app_with("DELIVERY:READ+EXECUTE", StubBehavior::Accept { run_id: "r".into() }).await;
        assert_eq!(
            app.oneshot(req("POST", "/delivery", r#"{"decompositionId":"d1","taskId":"t1","title":"x","executor":"GPT"}"#, Some(&t))).await.expect("r").status(),
            StatusCode::BAD_REQUEST
        );
    }
}
