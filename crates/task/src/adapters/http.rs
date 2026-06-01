//! 任务拆分的 HTTP 适配器。DTO ↔ 用例,命令错误映射 HTTP 码。
//!
//! RBAC(资源键 `TASK`):新建拆分/加任务需 `TASK:ADD`,派发(交给执行者)需 `TASK:EXECUTE`,
//! 状态流转需 `TASK:UPDATE`;读端点开放。错误码:校验→400,依赖未满足/非法流转→409,
//! 拆分/任务不存在→404。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use webauth::{AuthUser, SessionStore};

use crate::application::{
    BreakdownError, BreakdownUseCase, CreateDecompositionError, CreateDecompositionUseCase,
    TaskCmdError, TaskService,
};
use crate::domain::{Decomposition, Task, TaskStatus};
use crate::ports::RequirementSpec;

#[derive(Clone)]
struct TaskState {
    create: CreateDecompositionUseCase,
    breakdown: BreakdownUseCase,
    admin: TaskService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<TaskState> for Arc<dyn SessionStore> {
    fn from_ref(s: &TaskState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    create: CreateDecompositionUseCase,
    breakdown: BreakdownUseCase,
    admin: TaskService,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/decomposition", post(create_decomposition))
        .route("/decomposition/breakdown", post(breakdown_requirement))
        .route("/decomposition/{id}", get(get_decomposition))
        .route("/decomposition/{id}/ready", get(ready_tasks))
        .route("/decomposition/{id}/task", post(add_task))
        .route("/decomposition/{id}/task/{task_id}/dispatch", post(dispatch_task))
        .route("/decomposition/{id}/task/{task_id}/status", post(transition_task))
        .with_state(TaskState { create, breakdown, admin, sessions })
}

// ---- DTO ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBody {
    requirement_id: String,
    requirement_version: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BreakdownBody {
    requirement_id: String,
    requirement_version: u32,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
}

async fn breakdown_requirement(
    user: AuthUser,
    State(st): State<TaskState>,
    Json(b): Json<BreakdownBody>,
) -> Response {
    if !user.can("TASK", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let spec = RequirementSpec {
        requirement_id: b.requirement_id,
        requirement_version: b.requirement_version,
        title: b.title,
        description: b.description,
        acceptance_criteria: b.acceptance_criteria,
    };
    match st.breakdown.execute(&spec).await {
        Ok(d) => (StatusCode::CREATED, Json(DecompositionResponse::from(&d))).into_response(),
        Err(BreakdownError::EmptyRequirement) => (StatusCode::BAD_REQUEST, "requirement id required").into_response(),
        Err(BreakdownError::AlreadyExists) => (StatusCode::CONFLICT, "decomposition already exists").into_response(),
        Err(BreakdownError::Validation(_)) => (StatusCode::BAD_REQUEST, "invalid planned task").into_response(),
        Err(BreakdownError::Plan(_)) => (StatusCode::BAD_GATEWAY, "planner error").into_response(),
        Err(BreakdownError::Repo(_)) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddTaskBody {
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}

#[derive(Deserialize)]
struct StatusBody {
    status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskResponse {
    id: String,
    title: String,
    description: String,
    acceptance_criteria: Vec<String>,
    dependencies: Vec<String>,
    status: String,
}

impl From<&Task> for TaskResponse {
    fn from(t: &Task) -> Self {
        Self {
            id: t.id.clone(),
            title: t.title.clone(),
            description: t.description.clone(),
            acceptance_criteria: t.acceptance_criteria.clone(),
            dependencies: t.dependencies.clone(),
            status: t.status.as_str().to_string(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecompositionResponse {
    id: String,
    requirement_id: String,
    requirement_version: u32,
    complete: bool,
    ready_task_ids: Vec<String>,
    tasks: Vec<TaskResponse>,
}

impl From<&Decomposition> for DecompositionResponse {
    fn from(d: &Decomposition) -> Self {
        Self {
            id: d.id.clone(),
            requirement_id: d.requirement_id.clone(),
            requirement_version: d.requirement_version,
            complete: d.is_complete(),
            ready_task_ids: d.ready_tasks().iter().map(|t| t.id.clone()).collect(),
            tasks: d.tasks.iter().map(TaskResponse::from).collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskCreated {
    task_id: String,
}

// ---- 错误映射 ----

fn cmd_err(e: TaskCmdError) -> Response {
    match e {
        TaskCmdError::DecompositionNotFound => (StatusCode::NOT_FOUND, "decomposition not found").into_response(),
        TaskCmdError::TaskNotFound => (StatusCode::NOT_FOUND, "task not found").into_response(),
        TaskCmdError::Validation(_) => (StatusCode::BAD_REQUEST, "invalid task payload").into_response(),
        TaskCmdError::Conflict(_) => (StatusCode::CONFLICT, "task state conflict").into_response(),
        TaskCmdError::Repo(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

// ---- 处理器 ----

async fn create_decomposition(user: AuthUser, State(st): State<TaskState>, Json(b): Json<CreateBody>) -> Response {
    if !user.can("TASK", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.create.execute(&b.requirement_id, b.requirement_version).await {
        Ok(d) => (StatusCode::CREATED, Json(DecompositionResponse::from(&d))).into_response(),
        Err(CreateDecompositionError::EmptyRequirement) => {
            (StatusCode::BAD_REQUEST, "requirement id required").into_response()
        }
        Err(CreateDecompositionError::AlreadyExists) => {
            (StatusCode::CONFLICT, "decomposition already exists").into_response()
        }
        Err(CreateDecompositionError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

async fn get_decomposition(State(st): State<TaskState>, Path(id): Path<String>) -> Response {
    match st.admin.get(&id).await {
        Ok(d) => (StatusCode::OK, Json(DecompositionResponse::from(&d))).into_response(),
        Err(e) => cmd_err(e),
    }
}

async fn ready_tasks(State(st): State<TaskState>, Path(id): Path<String>) -> Response {
    match st.admin.get(&id).await {
        Ok(d) => {
            let ready: Vec<TaskResponse> = d.ready_tasks().into_iter().map(TaskResponse::from).collect();
            (StatusCode::OK, Json(ready)).into_response()
        }
        Err(e) => cmd_err(e),
    }
}

async fn add_task(
    user: AuthUser,
    State(st): State<TaskState>,
    Path(id): Path<String>,
    Json(b): Json<AddTaskBody>,
) -> Response {
    if !user.can("TASK", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.add_task(&id, &b.title, &b.description, &b.acceptance_criteria, &b.dependencies).await {
        Ok(task_id) => (StatusCode::CREATED, Json(TaskCreated { task_id })).into_response(),
        Err(e) => cmd_err(e),
    }
}

async fn dispatch_task(
    user: AuthUser,
    State(st): State<TaskState>,
    Path((id, task_id)): Path<(String, String)>,
) -> Response {
    if !user.can("TASK", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.dispatch(&id, &task_id).await {
        Ok(d) => (StatusCode::OK, Json(DecompositionResponse::from(&d))).into_response(),
        Err(e) => cmd_err(e),
    }
}

async fn transition_task(
    user: AuthUser,
    State(st): State<TaskState>,
    Path((id, task_id)): Path<(String, String)>,
    Json(b): Json<StatusBody>,
) -> Response {
    if !user.can("TASK", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let Some(to) = TaskStatus::parse(&b.status) else {
        return (StatusCode::BAD_REQUEST, "unknown status").into_response();
    };
    match st.admin.transition(&id, &task_id, to).await {
        Ok(d) => (StatusCode::OK, Json(DecompositionResponse::from(&d))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use crate::adapters::InMemoryTaskRepository;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app_with(perms: &str) -> (Router, String) {
        let repo = Arc::new(InMemoryTaskRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let set = PermissionSet::from_raw([perms.to_string()]).expect("perms");
        let token = sessions.create("u", set, 3600).await.expect("token");
        let r = router(
            CreateDecompositionUseCase::new(repo.clone()),
            crate::application::BreakdownUseCase::new(
                repo.clone(),
                Arc::new(crate::adapters::HeuristicPlanner),
            ),
            TaskService::new(repo),
            sessions,
        );
        (r, token)
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
    async fn dag_flow_create_add_dispatch_unlock() {
        let (app, t) = app_with("TASK:READ+ADD+EXECUTE+UPDATE").await;
        // 建拆分
        let r = app
            .clone()
            .oneshot(req("POST", "/decomposition", r#"{"requirementId":"req1","requirementVersion":1}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let did = json(r).await["id"].as_str().expect("id").to_string();

        // 加 A、B(B 依赖 A)
        let ra = app.clone().oneshot(req("POST", &format!("/decomposition/{did}/task"), r#"{"title":"A","acceptanceCriteria":["a1"]}"#, Some(&t))).await.expect("r");
        assert_eq!(ra.status(), StatusCode::CREATED);
        assert_eq!(json(ra).await["taskId"], "t1");
        let rb = app.clone().oneshot(req("POST", &format!("/decomposition/{did}/task"), r#"{"title":"B","dependencies":["t1"]}"#, Some(&t))).await.expect("r");
        assert_eq!(json(rb).await["taskId"], "t2");

        // ready 仅 t1
        let ready = app.clone().oneshot(req("GET", &format!("/decomposition/{did}/ready"), "", Some(&t))).await.expect("r");
        let arr = json(ready).await;
        assert_eq!(arr.as_array().expect("arr").len(), 1);
        assert_eq!(arr[0]["id"], "t1");

        // 派发 t2 → 依赖未满足 409
        assert_eq!(
            app.clone().oneshot(req("POST", &format!("/decomposition/{did}/task/t2/dispatch"), "", Some(&t))).await.expect("r").status(),
            StatusCode::CONFLICT
        );

        // 驱动 t1 → Verified
        for (uri, st) in [("dispatch", None), ("status", Some("RUNNING")), ("status", Some("DELIVERED")), ("status", Some("VERIFIED"))] {
            let (path, body) = match st {
                None => (format!("/decomposition/{did}/task/t1/dispatch"), String::new()),
                Some(s) => (format!("/decomposition/{did}/task/t1/status"), format!(r#"{{"status":"{s}"}}"#)),
            };
            let _ = uri;
            let resp = app.clone().oneshot(req("POST", &path, &body, Some(&t))).await.expect("r");
            assert_eq!(resp.status(), StatusCode::OK);
        }

        // 现在 t2 可派发
        let resp = app.oneshot(req("POST", &format!("/decomposition/{did}/task/t2/dispatch"), "", Some(&t))).await.expect("r");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json(resp).await;
        assert_eq!(v["tasks"][1]["status"], "DISPATCHED");
    }

    #[tokio::test]
    async fn rbac_create_requires_add_dispatch_requires_execute() {
        // 无令牌 → 401
        let (app, _t) = app_with("TASK:READ").await;
        assert_eq!(
            app.oneshot(req("POST", "/decomposition", r#"{"requirementId":"req1","requirementVersion":1}"#, None)).await.expect("r").status(),
            StatusCode::UNAUTHORIZED
        );
        // 只有 ADD,可建拆分但派发应 403
        let (app, t) = app_with("TASK:READ+ADD").await;
        let r = app.clone().oneshot(req("POST", "/decomposition", r#"{"requirementId":"req1","requirementVersion":1}"#, Some(&t))).await.expect("r");
        let did = json(r).await["id"].as_str().expect("id").to_string();
        app.clone().oneshot(req("POST", &format!("/decomposition/{did}/task"), r#"{"title":"A"}"#, Some(&t))).await.expect("r");
        assert_eq!(
            app.oneshot(req("POST", &format!("/decomposition/{did}/task/t1/dispatch"), "", Some(&t))).await.expect("r").status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn breakdown_builds_decomposition_via_planner() {
        let (app, t) = app_with("TASK:READ+ADD+EXECUTE+UPDATE").await;
        let r = app
            .clone()
            .oneshot(req("POST", "/decomposition/breakdown", r#"{"requirementId":"req1","requirementVersion":1,"title":"登录","acceptanceCriteria":["登录成功","错误密码拒绝"]}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = json(r).await;
        // 启发式:2 标准 + 集成验证 = 3 任务,集成依赖前两个
        assert_eq!(v["tasks"].as_array().expect("a").len(), 3);
        assert_eq!(v["tasks"][2]["dependencies"], serde_json::json!(["t1", "t2"]));
        assert_eq!(v["readyTaskIds"], serde_json::json!(["t1", "t2"]));
    }

    #[tokio::test]
    async fn breakdown_requires_add_permission() {
        let (app, t) = app_with("TASK:READ").await;
        assert_eq!(
            app.oneshot(req("POST", "/decomposition/breakdown", r#"{"requirementId":"req1","requirementVersion":1,"title":"x"}"#, Some(&t))).await.expect("r").status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn duplicate_decomposition_409() {
        let (app, t) = app_with("TASK:READ+ADD").await;
        let body = r#"{"requirementId":"req1","requirementVersion":1}"#;
        assert_eq!(
            app.clone().oneshot(req("POST", "/decomposition", body, Some(&t))).await.expect("r").status(),
            StatusCode::CREATED
        );
        assert_eq!(
            app.oneshot(req("POST", "/decomposition", body, Some(&t))).await.expect("r").status(),
            StatusCode::CONFLICT
        );
    }
}
