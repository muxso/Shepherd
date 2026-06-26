use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use api_runner::{Assertion, CaseOutcome, ReqwestRunner, RequestSpec};
use probe::{PluginRegistry, ProbeRequest};

#[derive(Clone)]
struct AgentState {
    runner: Arc<ReqwestRunner>,
    registry: Arc<PluginRegistry>,
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunBody {
    request: RequestSpec,
    #[serde(default)]
    assertions: Vec<Assertion>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunResult {
    outcome: String,
    status: Option<u16>,
    elapsed_ms: Option<u64>,
    failures: Vec<String>,
}

async fn healthz() -> &'static str {
    "ok"
}

async fn run(State(st): State<AgentState>, headers: HeaderMap, Json(body): Json<RunBody>) -> Response {
    if let Some(expected) = &st.token {
        let ok = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|t| t == expected)
            .unwrap_or(false);
        if !ok {
            return (StatusCode::UNAUTHORIZED, "missing or bad token").into_response();
        }
    }

    let (report, snapshot) =
        st.runner.run_case_with_snapshot(&body.request, &body.assertions).await;
    let outcome = match report.outcome {
        CaseOutcome::Success => "SUCCESS",
        CaseOutcome::Error => "ERROR",
    };
    Json(RunResult {
        outcome: outcome.to_string(),
        status: snapshot.as_ref().map(|s| s.status),
        elapsed_ms: snapshot.as_ref().map(|s| s.elapsed_ms),
        failures: report.failures,
    })
    .into_response()
}

fn authorized(token: &Option<String>, headers: &HeaderMap) -> bool {
    match token {
        None => true,
        Some(expected) => headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|t| t == expected)
            .unwrap_or(false),
    }
}

async fn probe(State(st): State<AgentState>, headers: HeaderMap, Json(req): Json<ProbeRequest>) -> Response {
    if !authorized(&st.token, &headers) {
        return (StatusCode::UNAUTHORIZED, "missing or bad token").into_response();
    }
    Json(st.registry.dispatch(&req).await).into_response()
}

async fn protocols(State(st): State<AgentState>) -> Response {
    Json(st.registry.protocols()).into_response()
}

fn app(state: AgentState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/run", post(run))
        .route("/probe", post(probe))
        .route("/protocols", get(protocols))
        .with_state(state)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let bind = std::env::var("RUNNER_BIND").unwrap_or_else(|_| "0.0.0.0:9100".to_string());
    let token = std::env::var("RUNNER_TOKEN").ok().filter(|t| !t.trim().is_empty());
    let registry = Arc::new(probe::default_registry());
    let state = AgentState { runner: Arc::new(ReqwestRunner::no_proxy()), registry, token };

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, auth = state.token.is_some(), protocols = ?state.registry.protocols(), "runner-agent listening");
    axum::serve(listener, app(state)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    async fn json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    fn run_req(body: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method("POST").uri("/run").header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    fn state(token: Option<&str>) -> AgentState {
        AgentState {
            runner: Arc::new(ReqwestRunner::no_proxy()),
            registry: Arc::new(probe::default_registry()),
            token: token.map(|t| t.to_string()),
        }
    }

    #[tokio::test]
    async fn transport_failure_is_error_outcome() {
        let resp = app(state(None))
            .oneshot(run_req(
                r#"{"request":{"method":"GET","url":"http://127.0.0.1:1/nope","headers":[],"body":null},"assertions":[{"type":"StatusIs","args":200}]}"#,
                None,
            ))
            .await
            .expect("resp");
        let v = json(resp).await;
        assert_eq!(v["outcome"], "ERROR");
        assert!(v["failures"].as_array().expect("arr").len() >= 1);
    }

    #[tokio::test]
    async fn protocols_lists_enabled_plugins() {
        let resp = app(state(None))
            .oneshot(Request::builder().uri("/protocols").body(Body::empty()).expect("req"))
            .await
            .expect("resp");
        let v = json(resp).await;
        let names: Vec<String> =
            v.as_array().expect("arr").iter().map(|s| s.as_str().unwrap_or("").to_string()).collect();
        assert!(names.contains(&"http".to_string()));
        assert!(names.contains(&"grpc".to_string()));
        assert!(names.contains(&"sql".to_string()));
    }

    #[tokio::test]
    async fn probe_http_transport_failure() {
        let body = r#"{"protocol":"http","target":"http://127.0.0.1:1/nope","assertions":[{"type":"success"}]}"#;
        let req = Request::builder()
            .method("POST")
            .uri("/probe")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .expect("req");
        let resp = app(state(None)).oneshot(req).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json(resp).await;
        assert_eq!(v["success"], false);
        assert!(v["failures"].as_array().expect("arr").len() >= 1);
    }

    #[tokio::test]
    async fn probe_unsupported_protocol() {
        let body = r#"{"protocol":"smtp","target":"x","assertions":[]}"#;
        let req = Request::builder()
            .method("POST")
            .uri("/probe")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .expect("req");
        let v = json(app(state(None)).oneshot(req).await.expect("resp")).await;
        assert_eq!(v["success"], false);
        assert!(v["failures"][0].as_str().unwrap_or("").contains("unsupported protocol"));
    }

    #[tokio::test]
    async fn auth_required_when_token_set() {
        let body = r#"{"request":{"method":"GET","url":"http://127.0.0.1:1/x","headers":[],"body":null}}"#;
        let resp =
            app(state(Some("secret"))).oneshot(run_req(body, None)).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let resp = app(state(Some("secret")))
            .oneshot(run_req(body, Some("secret")))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn healthz_ok() {
        let resp = app(state(None))
            .oneshot(Request::builder().uri("/healthz").body(Body::empty()).expect("req"))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
