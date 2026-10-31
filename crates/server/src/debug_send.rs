use std::sync::Arc;

use axum::{
    extract::{FromRef, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use webauth::{AuthUser, SessionStore};

use api_runner::{
    evaluate_detailed_with_vars, run_extracts, Assertion, AssertionReport, HttpMethod, Processor,
    RequestSpec, ReqwestRunner, ResponseSnapshot,
};
use probe::{default_registry, ProbeRequest};

#[derive(Clone)]
struct DebugState {
    sessions: Arc<dyn SessionStore>,
    environment: crate::environment_extension::ExecutionResolver,
}

impl FromRef<DebugState> for Arc<dyn SessionStore> {
    fn from_ref(s: &DebugState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    sessions: Arc<dyn SessionStore>,
    environment: crate::environment_extension::ExecutionResolver,
) -> Router {
    Router::new()
        .route("/api/debug/send", post(send))
        .route("/api/debug/protocols", get(protocols))
        .with_state(DebugState { sessions, environment })
}

async fn protocols(_user: AuthUser) -> Response {
    let mut list = default_registry().protocols();
    if !list.iter().any(|p| p == "http") {
        list.push("http".to_string());
    }
    list.sort();
    (StatusCode::OK, Json(serde_json::json!({ "protocols": list }))).into_response()
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct HeaderKv {
    key: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendBody {
    #[serde(default)]
    environment_id: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    variables: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pre_processors: Vec<Processor>,
    #[serde(default)]
    query: Vec<HeaderKv>,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default = "default_method")]
    method: String,
    url: String,
    #[serde(default)]
    headers: Vec<HeaderKv>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    meta: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    assertions: Vec<Assertion>,
    #[serde(default)]
    processors: Vec<Processor>,
}

fn default_method() -> String {
    "GET".to_string()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssertionResult {
    item: String,
    condition: String,
    expected: String,
    actual: String,
    passed: bool,
    reason: String,
}

impl From<AssertionReport> for AssertionResult {
    fn from(r: AssertionReport) -> Self {
        Self {
            item: r.item,
            condition: r.condition,
            expected: r.expected,
            actual: r.actual,
            passed: r.passed,
            reason: r.reason,
        }
    }
}

#[derive(Serialize)]
struct RequestSnapshot {
    method: HttpMethod,
    url: String,
    headers: Vec<HeaderKv>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SendResponse {
    status: u16,
    latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    timings: Option<api_runner::PhaseTimings>,
    headers: Vec<(String, String)>,
    body: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    assertions: Vec<AssertionResult>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    extractions: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    preparation: Vec<api_runner::PreparationStepReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request: Option<RequestSnapshot>,
}

async fn send(
    user: AuthUser,
    State(state): State<DebugState>,
    Json(req): Json<SendBody>,
) -> Response {
    if req.url.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "url required").into_response();
    }
    if req.variables.len() > 128
        || req.variables.iter().any(|(k, v)| k.is_empty() || k.len() > 128 || v.len() > 65_536)
        || req.variables.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>() > 65_536
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"preparation variables exceed limits"})),
        )
            .into_response();
    }
    if req.processors.iter().any(|p| !matches!(p, Processor::Extract { .. })) {
        return (StatusCode::BAD_REQUEST,Json(serde_json::json!({"error":"this post-processor is not supported yet; preparation belongs in preProcessors"}))).into_response();
    }
    let proto = req.protocol.as_deref().unwrap_or("http").trim().to_lowercase();
    if !proto.is_empty() && proto != "http" {
        if !req.pre_processors.is_empty() {
            return (StatusCode::BAD_REQUEST,Json(serde_json::json!({"error":"preparation currently supports server-side HTTP debugging"}))).into_response();
        }
        let reg = default_registry();
        if !reg.protocols().iter().any(|p| p == &proto) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("unsupported protocol: {proto}") })),
            )
                .into_response();
        }
        let preq = ProbeRequest {
            protocol: proto,
            target: req.url.clone(),
            payload: req.body.clone(),
            metadata: req.meta.clone(),
            assertions: Vec::new(),
        };
        let out = reg.dispatch(&preq).await;
        return if out.success {
            (
                StatusCode::OK,
                Json(SendResponse {
                    status: out.status.unwrap_or(0) as u16,
                    latency_ms: out.latency_ms,
                    timings: None,
                    headers: Vec::new(),
                    body: out.output.unwrap_or_default(),
                    assertions: Vec::new(),
                    extractions: Vec::new(),
                    preparation: Vec::new(),
                    request: None,
                }),
            )
                .into_response()
        } else {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": out.failures.join("; "),
                    "output": out.output,
                })),
            )
                .into_response()
        };
    }
    let method: HttpMethod =
        match serde_json::from_value(serde_json::Value::String(req.method.to_uppercase())) {
            Ok(method) => method,
            Err(_) => return (StatusCode::BAD_REQUEST, "unsupported HTTP method").into_response(),
        };
    let prepared = match state
        .environment
        .prepare(user.clone(), req.environment_id.as_deref(), req.variables, &req.pre_processors)
        .await
    {
        Ok(prepared) => prepared,
        Err(response) => return response,
    };
    let spec = RequestSpec {
        method,
        url: req.url.clone(),
        headers: req.headers.into_iter().map(|h| (h.key, h.value)).collect(),
        body: req.body,
    };
    let query: Vec<_> = req.query.into_iter().map(|q| (q.key, q.value)).collect();
    let spec = match api_runner::domain::preparation::prepare_request(
        spec,
        req.base_url.as_deref(),
        &query,
        &prepared.variables,
        1_048_576,
    ) {
        Ok(spec) => spec,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":error.to_string()})))
                .into_response()
        }
    };
    let hosts = match req.environment_id.as_deref() {
        Some(environment) => match state.environment.host_overrides(user, environment).await {
            Ok(hosts) => hosts,
            Err(response) => return response,
        },
        None => api_runner::HostOverrides::default(),
    };
    let runner = match ReqwestRunner::with_host_overrides(&hosts) {
        Ok(runner) => runner,
        Err(error) => {
            return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error":error.message()})))
                .into_response()
        }
    };
    match runner.execute(&spec).await {
        Ok(s) => {
            let snapshot = ResponseSnapshot {
                status: s.status,
                headers: s.headers,
                body: s.body,
                elapsed_ms: s.elapsed_ms,
                timings: s.timings,
            };
            let assertions: Vec<AssertionResult> = if req.assertions.is_empty() {
                Vec::new()
            } else {
                evaluate_detailed_with_vars(&req.assertions, &snapshot, &prepared.variables)
                    .into_iter()
                    .map(Into::into)
                    .collect()
            };
            let extractions = if req.processors.is_empty() {
                Vec::new()
            } else {
                run_extracts(&req.processors, &snapshot)
            };
            (
                StatusCode::OK,
                Json(SendResponse {
                    status: snapshot.status,
                    latency_ms: snapshot.elapsed_ms,
                    timings: Some(snapshot.timings),
                    headers: snapshot.headers,
                    body: snapshot.body,
                    assertions,
                    extractions,
                    preparation: prepared.steps,
                    request: Some(RequestSnapshot {
                        method: spec.method,
                        url: spec.url,
                        headers: spec
                            .headers
                            .into_iter()
                            .map(|(key, value)| HeaderKv { key, value })
                            .collect(),
                        body: spec.body,
                    }),
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("{}: {}", e.kind(), e.message()) })),
        )
            .into_response(),
    }
}
