//! 组装根桥:`POST /api/debug/send` —— 进程内即时发起一次 HTTP 请求并回传响应。
//!
//! 供前端「发送/响应」调试台直连(不经 runner-agent):复用 api-runner 的 reqwest 执行器,
//! 支持自定义 method/headers/body,回传 status/耗时/响应头/响应体。只读目标,RBAC 仅要求登录。

use std::sync::Arc;

use axum::{
    extract::FromRef,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use webauth::{AuthUser, SessionStore};

use api_runner::{HttpMethod, ReqwestRunner, RequestSpec};
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
    Router::new().route("/api/debug/send", post(send)).with_state(DebugState { sessions })
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
    /// 协议:缺省/HTTP 走进程内 reqwest;其余(redis/ssh/…)走 probe 插件就地执行。
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default = "default_method")]
    method: String,
    /// HTTP 为 URL;非 HTTP 为连接目标(如 redis://host:port/0、ssh://user@host:port)。
    url: String,
    #[serde(default)]
    headers: Vec<HeaderKv>,
    /// HTTP 为请求体;非 HTTP 为载荷(redis 命令行 / ssh 命令)。
    #[serde(default)]
    body: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SendResponse {
    status: u16,
    latency_ms: u64,
    headers: Vec<(String, String)>,
    body: String,
}

async fn send(_user: AuthUser, Json(req): Json<SendBody>) -> Response {
    if req.url.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "url required").into_response();
    }
    // 非 HTTP 协议:走 probe 插件(redis/ssh/…),就地执行一次并回传统一响应。
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
            metadata: Default::default(),
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
        Ok(s) => (
            StatusCode::OK,
            Json(SendResponse {
                status: s.status,
                latency_ms: s.elapsed_ms,
                headers: s.headers,
                body: s.body,
            }),
        )
            .into_response(),
        // 传输失败(DNS/连接/超时):回 502 + 错误信息,前端调试台展示。
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("{}: {}", e.kind(), e.message()) })),
        )
            .into_response(),
    }
}
