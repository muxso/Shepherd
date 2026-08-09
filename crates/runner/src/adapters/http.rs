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
use probe::{ProbeAssertion, ProbeRequest};

use crate::application::{
    RegisterError, RunCaseError, RunProbeError, RunViaAgentError, RunnerService,
};
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
        .route("/runner-agent/{id}/run-case", post(run_case))
        .route("/runner-agent/{id}/refresh", post(refresh))
        .route("/runner-agent/{id}/executions", get(executions))
        .route("/runner/probe", post(run_probe))
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
    protocols: Vec<String>,
}

impl From<RunnerAgent> for AgentResponse {
    fn from(a: RunnerAgent) -> Self {
        Self {
            id: a.id,
            name: a.name,
            base_url: a.base_url,
            enabled: a.enabled,
            protocols: a.protocols,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunViaBody {
    #[schema(value_type = Object)]
    request: RequestSpec,
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    assertions: Vec<Assertion>,
}

#[utoipa::path(post, path = "/runner-agent", tag = "runner", request_body = RegisterBody, responses((status = 201, body = AgentResponse), (status = 400), (status = 403)), security(("bearer" = [])))]
async fn register(
    user: AuthUser,
    State(st): State<RunnerState>,
    Json(b): Json<RegisterBody>,
) -> Response {
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

#[utoipa::path(get, path = "/runner-agent", tag = "runner", responses((status = 200, body = [AgentResponse]), (status = 403)), security(("bearer" = [])))]
async fn list(user: AuthUser, State(st): State<RunnerState>) -> Response {
    if !user.can("RUNNER", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
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
        // Agent unreachable/errored: 502 (fault is on the target-environment agent, not central).
        Err(RunViaAgentError::Backend(_)) => {
            (StatusCode::BAD_GATEWAY, "agent dispatch failed").into_response()
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunCaseBody {
    case_id: String,
}

#[utoipa::path(post, path = "/runner-agent/{id}/run-case", tag = "runner", params(("id" = String, Path)), request_body = RunCaseBody, responses((status = 200), (status = 404), (status = 502)), security(("bearer" = [])))]
async fn run_case(
    user: AuthUser,
    State(st): State<RunnerState>,
    Path(id): Path<String>,
    Json(b): Json<RunCaseBody>,
) -> Response {
    if !user.can("RUNNER", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.run_case(&id, &b.case_id).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(RunCaseError::AgentNotFound) => {
            (StatusCode::NOT_FOUND, "agent not found or disabled").into_response()
        }
        Err(RunCaseError::CaseNotFound) => {
            (StatusCode::NOT_FOUND, "case not found").into_response()
        }
        Err(RunCaseError::Backend(_)) => {
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

#[utoipa::path(get, path = "/runner-agent/{id}/executions", tag = "runner", params(("id" = String, Path)), responses((status = 200, body = [ExecutionResponse]), (status = 403)), security(("bearer" = [])))]
async fn executions(
    user: AuthUser,
    State(st): State<RunnerState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("RUNNER", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.executions(&id, 50).await {
        Ok(list) => {
            let items: Vec<ExecutionResponse> =
                list.into_iter().map(ExecutionResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(post, path = "/runner-agent/{id}/refresh", tag = "runner", params(("id" = String, Path)), responses((status = 200), (status = 404), (status = 502)), security(("bearer" = [])))]
async fn refresh(
    user: AuthUser,
    State(st): State<RunnerState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("RUNNER", "EDIT") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.refresh_capabilities(&id).await {
        Ok(protocols) => (StatusCode::OK, Json(protocols)).into_response(),
        Err(RunViaAgentError::AgentNotFound) => {
            (StatusCode::NOT_FOUND, "agent not found or disabled").into_response()
        }
        Err(RunViaAgentError::Backend(_)) => {
            (StatusCode::BAD_GATEWAY, "agent unreachable").into_response()
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ProbeBody {
    protocol: String,
    target: String,
    #[serde(default)]
    payload: Option<String>,
    #[serde(default)]
    #[schema(value_type = Object)]
    metadata: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    assertions: Vec<ProbeAssertion>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ProbeReportResponse {
    agent_id: String,
    agent_name: String,
    #[schema(value_type = Object)]
    outcome: probe::ProbeOutcome,
}

#[utoipa::path(post, path = "/runner/probe", tag = "runner", request_body = ProbeBody, responses((status = 200, body = ProbeReportResponse), (status = 404), (status = 502)), security(("bearer" = [])))]
async fn run_probe(
    user: AuthUser,
    State(st): State<RunnerState>,
    Json(b): Json<ProbeBody>,
) -> Response {
    if !user.can("RUNNER", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let req = ProbeRequest {
        protocol: b.protocol,
        target: b.target,
        payload: b.payload,
        metadata: b.metadata,
        assertions: b.assertions,
    };
    match st.svc.run_probe(&req).await {
        Ok(report) => (
            StatusCode::OK,
            Json(ProbeReportResponse {
                agent_id: report.agent_id,
                agent_name: report.agent_name,
                outcome: report.outcome,
            }),
        )
            .into_response(),
        // No agent supports this protocol: 404 (central has no routable target).
        Err(RunProbeError::NoAgent(_)) => {
            (StatusCode::NOT_FOUND, "no agent supports this protocol").into_response()
        }
        Err(RunProbeError::Backend(_)) => {
            (StatusCode::BAD_GATEWAY, "agent dispatch failed").into_response()
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(register, list, run_via, run_case, refresh, executions, run_probe),
    components(schemas(
        RegisterBody, AgentResponse, RunViaBody, RunCaseBody, ExecutionResponse, ProbeBody,
        ProbeReportResponse
    )),
    tags((name = "runner", description = "Remote execution agent"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{
        InMemoryAgentStore, InMemoryCaseSpecSource, InMemoryExecutionStore, StubCapabilities,
        StubRemoteProbe, StubRemoteRunner,
    };
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app_full(perms: &str) -> (Router, String, Arc<InMemoryCaseSpecSource>) {
        let (app, token, _caps, cases) = app_with_caps(perms).await;
        (app, token, cases)
    }

    async fn app_with_caps(
        perms: &str,
    ) -> (Router, String, Arc<StubCapabilities>, Arc<InMemoryCaseSpecSource>) {
        let store = Arc::new(InMemoryAgentStore::new());
        let remote = Arc::new(StubRemoteRunner::success());
        let probe = Arc::new(StubRemoteProbe);
        let caps = Arc::new(StubCapabilities::new());
        let execs = Arc::new(InMemoryExecutionStore::new());
        let cases = Arc::new(InMemoryCaseSpecSource::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let set = PermissionSet::from_raw([perms.to_string()]).expect("perms");
        let token = sessions.create("u", set, 3600).await.expect("token");
        let svc = RunnerService::new(store, remote, probe, caps.clone(), execs, cases.clone());
        (router(svc, sessions), token, caps, cases)
    }

    async fn app(perms: &str) -> (Router, String) {
        let (r, t, _c) = app_full(perms).await;
        (r, t)
    }

    fn post(uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b =
            Request::builder().method("POST").uri(uri).header("content-type", "application/json");
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
                r#"{"name":"test environment","baseUrl":"http://10.0.0.5:9100","token":"s"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let id = json(r).await["id"].as_str().expect("id").to_string();

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

        let r = app
            .oneshot(
                Request::builder()
                    .uri(format!("/runner-agent/{id}/executions"))
                    .header("authorization", format!("Bearer {t}"))
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
    async fn run_stored_case_via_agent() {
        use api_runner::{Assertion, HttpMethod, RequestSpec};
        let (app, t, cases) = app_full("RUNNER:READ+ADD+EXECUTE").await;
        cases.seed(
            "case1",
            RequestSpec {
                method: HttpMethod::Get,
                url: "http://t/x".into(),
                headers: vec![],
                body: None,
            },
            vec![Assertion::StatusIs(200)],
        );
        let r = app
            .clone()
            .oneshot(post(
                "/runner-agent",
                r#"{"name":"environment A","baseUrl":"http://a:9100"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        let id = json(r).await["id"].as_str().expect("id").to_string();

        let r = app
            .oneshot(post(
                &format!("/runner-agent/{id}/run-case"),
                r#"{"caseId":"case1"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(json(r).await["outcome"], "SUCCESS");
    }

    #[tokio::test]
    async fn run_case_unknown_case_404() {
        let (app, t) = app("RUNNER:READ+ADD+EXECUTE").await;
        let r = app
            .clone()
            .oneshot(post("/runner-agent", r#"{"name":"e","baseUrl":"http://a"}"#, Some(&t)))
            .await
            .expect("r");
        let id = json(r).await["id"].as_str().expect("id").to_string();
        let r = app
            .oneshot(post(
                &format!("/runner-agent/{id}/run-case"),
                r#"{"caseId":"ghost"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
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
    async fn probe_routes_by_protocol_to_matching_agent() {
        let (app, t, caps, _c) = app_with_caps("RUNNER:READ+ADD+EXECUTE").await;
        caps.set("http://grpc-env:9100", &["http", "grpc"]);
        caps.set("http://sql-env:9100", &["http", "sql"]);
        for (name, url) in [
            ("gRPC environment", "http://grpc-env:9100"),
            ("SQL environment", "http://sql-env:9100"),
        ] {
            app.clone()
                .oneshot(post(
                    "/runner-agent",
                    &format!(r#"{{"name":"{name}","baseUrl":"{url}"}}"#),
                    Some(&t),
                ))
                .await
                .expect("reg");
        }

        let r = app
            .clone()
            .oneshot(post(
                "/runner/probe",
                r#"{"protocol":"grpc","target":"grpc-env:50051","assertions":[{"type":"success"}]}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::OK);
        let v = json(r).await;
        assert_eq!(v["agentName"], "gRPC environment");
        assert_eq!(v["outcome"]["success"], true);

        let r = app
            .clone()
            .oneshot(post("/runner/probe", r#"{"protocol":"sql","target":"x"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(json(r).await["agentName"], "SQL environment");

        let r = app
            .oneshot(post("/runner/probe", r#"{"protocol":"redis","target":"x"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn probe_requires_execute() {
        let (app, t, _caps, _c) = app_with_caps("RUNNER:READ").await;
        let r = app
            .oneshot(post("/runner/probe", r#"{"protocol":"http","target":"x"}"#, Some(&t)))
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
