//! 中央侧 runner 闭环 HTTP 适配器:注册/列出 agent + 把用例派给 agent 执行。
//!
//! RBAC 资源串 `RUNNER`:注册需 ADD、派发执行需 EXECUTE;列出开放。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use api_runner::{Assertion, RequestSpec};

use crate::application::{RegisterError, RunViaAgentError, RunnerService};
use crate::domain::{ExecutionRecord, RunnerAgent};

#[derive(Clone)]
struct RunnerState {
    svc: RunnerService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<RunnerState> for Arc<dyn SessionStore> {
    fn from_ref(s: &RunnerState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(svc: RunnerService, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/runner-agent", post(register).get(list))
        .route("/runner-agent/{id}/run", post(run_via))
        .route("/runner-agent/{id}/executions", get(executions))
        .with_state(RunnerState { svc, sessions })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RegisterBody {
    name: String,
    base_url: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct AgentResponse {
    id: String,
    name: String,
    base_url: String,
    enabled: bool,
}

impl From<RunnerAgent> for AgentResponse {
    fn from(a: RunnerAgent) -> Self {
        Self { id: a.id, name: a.name, base_url: a.base_url, enabled: a.enabled }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunViaBody {
    /// 自包含请求规格(api-runner RequestSpec;OpenAPI 视为不透明对象)。
    #[schema(value_type = Object)]
    request: RequestSpec,
    /// 断言列表(api-runner Assertion)。
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    assertions: Vec<Assertion>,
}

#[utoipa::path(post, path = "/runner-agent", tag = "runner", request_body = RegisterBody, responses((status = 201, body = AgentResponse), (status = 400), (status = 403)), security(("bearer" = [])))]
async fn register(user: AuthUser, State(st): State<RunnerState>, Json(b): Json<RegisterBody>) -> Response {
    if !user.can("RUNNER", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.register(&b.name, &b.base_url, b.token, b.enabled).await {
        Ok(a) => (StatusCode::CREATED, Json(AgentResponse::from(a))).into_response(),
        Err(RegisterError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid agent payload").into_response()
        }
        Err(RegisterError::Backend(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/runner-agent", tag = "runner", responses((status = 200, body = [AgentResponse])))]
async fn list(State(st): State<RunnerState>) -> Response {
    match st.svc.list().await {
        Ok(list) => {
            let items: Vec<AgentResponse> = list.into_iter().map(AgentResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(post, path = "/runner-agent/{id}/run", tag = "runner", params(("id" = String, Path)), request_body = RunViaBody, responses((status = 200), (status = 404), (status = 502)), security(("bearer" = [])))]
async fn run_via(
    user: AuthUser,
    State(st): State<RunnerState>,
    Path(id): Path<String>,
    Json(b): Json<RunViaBody>,
) -> Response {
    if !user.can("RUNNER", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.run_via(&id, &b.request, &b.assertions).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(RunViaAgentError::AgentNotFound) => {
            (StatusCode::NOT_FOUND, "agent not found or disabled").into_response()
        }
        // agent 不可达/出错:上游 502(中央侧无误,是目标环境的 agent 问题)。
        Err(RunViaAgentError::Backend(_)) => {
            (StatusCode::BAD_GATEWAY, "agent dispatch failed").into_response()
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ExecutionResponse {
    id: String,
    agent_id: String,
    method: String,
    url: String,
    outcome: String,
    status: Option<u16>,
    elapsed_ms: Option<u64>,
    failures: Vec<String>,
    executed_at: String,
}

impl From<ExecutionRecord> for ExecutionResponse {
    fn from(e: ExecutionRecord) -> Self {
        Self {
            id: e.id,
            agent_id: e.agent_id,
            method: e.method,
            url: e.url,
            outcome: e.outcome,
            status: e.status,
            elapsed_ms: e.elapsed_ms,
            failures: e.failures,
            executed_at: e.executed_at,
        }
    }
}

#[utoipa::path(get, path = "/runner-agent/{id}/executions", tag = "runner", params(("id" = String, Path)), responses((status = 200, body = [ExecutionResponse])))]
async fn executions(State(st): State<RunnerState>, Path(id): Path<String>) -> Response {
    match st.svc.executions(&id, 50).await {
        Ok(list) => {
            let items: Vec<ExecutionResponse> =
                list.into_iter().map(ExecutionResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(register, list, run_via, executions),
    components(schemas(RegisterBody, AgentResponse, RunViaBody, ExecutionResponse)),
    tags((name = "runner", description = "远程执行 agent"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{InMemoryAgentStore, InMemoryExecutionStore, StubRemoteRunner};
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app(perms: &str) -> (Router, String) {
        let store = Arc::new(InMemoryAgentStore::new());
        let remote = Arc::new(StubRemoteRunner::success());
        let execs = Arc::new(InMemoryExecutionStore::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let set = PermissionSet::from_raw([perms.to_string()]).expect("perms");
        let token = sessions.create("u", set, 3600).await.expect("token");
        (router(RunnerService::new(store, remote, execs), sessions), token)
    }

    fn post(uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method("POST").uri(uri).header("content-type", "application/json");
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
    async fn register_then_run_via_agent() {
        let (app, t) = app("RUNNER:READ+ADD+EXECUTE").await;
        let r = app
            .clone()
            .oneshot(post(
                "/runner-agent",
                r#"{"name":"测试环境","baseUrl":"http://10.0.0.5:9100","token":"s"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let id = json(r).await["id"].as_str().expect("id").to_string();

        // 派用例给该 agent(桩远程 → SUCCESS)
        let r = app
            .clone()
            .oneshot(post(
                &format!("/runner-agent/{id}/run"),
                r#"{"request":{"method":"GET","url":"http://t/x","headers":[],"body":null},"assertions":[{"type":"StatusIs","args":200}]}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(json(r).await["outcome"], "SUCCESS");

        // 执行历史:刚那次派发应入档。
        let r = app
            .oneshot(
                Request::builder()
                    .uri(format!("/runner-agent/{id}/executions"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::OK);
        let v = json(r).await;
        assert_eq!(v.as_array().expect("arr").len(), 1);
        assert_eq!(v[0]["outcome"], "SUCCESS");
        assert_eq!(v[0]["method"], "GET");
    }

    #[tokio::test]
    async fn register_requires_add() {
        let (app, t) = app("RUNNER:READ").await;
        let r = app
            .oneshot(post("/runner-agent", r#"{"name":"e","baseUrl":"http://x"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn run_via_unknown_agent_404() {
        let (app, t) = app("RUNNER:READ+ADD+EXECUTE").await;
        let r = app
            .oneshot(post(
                "/runner-agent/ghost/run",
                r#"{"request":{"method":"GET","url":"http://t","headers":[],"body":null}}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
    }
}
