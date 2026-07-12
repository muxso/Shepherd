use std::sync::Arc;

use axum::{
    extract::FromRef,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use webauth::{AuthUser, SessionStore};

use api_runner::{
    evaluate_detailed, run_extracts, Assertion, AssertionReport, HttpMethod, Processor,
    RequestSpec, ReqwestRunner, ResponseSnapshot,
};
use probe::{default_registry, ProbeRequest};

#[derive(Clone)]
struct DebugState {
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<DebugState> for Arc<dyn SessionStore> {
    fn from_ref(s: &DebugState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/api/debug/send", post(send))
        .route("/api/debug/protocols", get(protocols))
        .with_state(DebugState { sessions })
}

async fn protocols(_user: AuthUser) -> Response {
    let mut list = default_registry().protocols();
    if !list.iter().any(|p| p == "http") {
        list.push("http".to_string());
    }
    list.sort();
    (StatusCode::OK, Json(serde_json::json!({ "protocols": list }))).into_response()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeaderKv {
    key: String,
    value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendBody {
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SendResponse {
    status: u16,
    latency_ms: u64,
    headers: Vec<(String, String)>,
    body: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    assertions: Vec<AssertionResult>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    extractions: Vec<(String, String)>,
}

async fn send(_user: AuthUser, Json(req): Json<SendBody>) -> Response {
    if req.url.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "url required").into_response();
    }
    let proto = req.protocol.as_deref().unwrap_or("http").trim().to_lowercase();
    if !proto.is_empty() && proto != "http" {
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
                    headers: Vec::new(),
                    body: out.output.unwrap_or_default(),
                    assertions: Vec::new(),
                    extractions: Vec::new(),
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
        serde_json::from_value(serde_json::Value::String(req.method.to_uppercase()))
            .unwrap_or(HttpMethod::Get);
    let spec = RequestSpec {
        method,
        url: req.url.clone(),
        headers: req.headers.into_iter().map(|h| (h.key, h.value)).collect(),
        body: req.body,
    };
    match ReqwestRunner::no_proxy().execute(&spec).await {
        Ok(s) => {
            let snapshot = ResponseSnapshot {
                status: s.status,
                headers: s.headers,
                body: s.body,
                elapsed_ms: s.elapsed_ms,
            };
            let assertions: Vec<AssertionResult> = if req.assertions.is_empty() {
                Vec::new()
            } else {
                evaluate_detailed(&req.assertions, &snapshot).into_iter().map(Into::into).collect()
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
                    headers: snapshot.headers,
                    body: snapshot.body,
                    assertions,
                    extractions,
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
