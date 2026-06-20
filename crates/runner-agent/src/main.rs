//! runner-agent —— 独立可部署的接口执行 agent。
//!
//! 中央 Shepherd 把**自包含**的执行请求(完整 RequestSpec + 断言)POST 给本 agent,
//! agent 用 `api-runner` 就地执行(它部署在目标环境内,能连到该环境的被测服务),回传结果。
//! 不依赖中央数据库/会话 —— 因此验证某个隔离环境时,只需在那个环境部署这个小二进制。
//!
//! 端点:
//!   GET  /healthz        存活探针
//!   POST /run            执行一条用例,返回 {outcome,status,elapsedMs,failures}
//! 鉴权:设了 `RUNNER_TOKEN` 则 `/run` 需 `Authorization: Bearer <token>`。
//! 绑定:`RUNNER_BIND`(默认 0.0.0.0:9100)。

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

#[derive(Clone)]
struct AgentState {
    runner: Arc<ReqwestRunner>,
    /// 设了则要求 Bearer 鉴权。
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunBody {
    /// 自包含请求规格(method/url/headers/body)。
    request: RequestSpec,
    /// 断言列表(可空 = 仅看传输是否成功)。
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
    // 鉴权(配置了 token 才校验)。
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

fn app(state: AgentState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/run", post(run))
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
    // no_proxy:直连目标环境的被测主机,不被全局代理劫持。
    let state = AgentState { runner: Arc::new(ReqwestRunner::no_proxy()), token };

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, auth = state.token.is_some(), "runner-agent listening");
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
            token: token.map(|t| t.to_string()),
        }
    }

    #[tokio::test]
    async fn transport_failure_is_error_outcome() {
        // 连不上的目标 → ERROR(传输失败),验证 agent 就地执行并如实回传。
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
    async fn auth_required_when_token_set() {
        let body = r#"{"request":{"method":"GET","url":"http://127.0.0.1:1/x","headers":[],"body":null}}"#;
        // 无 token → 401
        let resp =
            app(state(Some("secret"))).oneshot(run_req(body, None)).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        // 带正确 token → 放行(执行,传输失败但非 401)
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
