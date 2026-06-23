//! axum 兜底适配器:任意进入的请求 → 翻译成 [`MockRequest`] → 匹配规则 → 回放响应。
//!
//! 本层只做 DTO 翻译 + 响应组装;匹配/优先级全在 domain。无命中 → 404。

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Router,
};

use crate::domain::{match_request, MockRequest, MockResponse};
use crate::ports::MockRuleSource;

#[derive(Clone)]
struct MockState {
    source: Arc<dyn MockRuleSource>,
}

/// 装配 Mock 兜底路由:所有路径/方法都进 `handle`。
pub fn router(source: Arc<dyn MockRuleSource>) -> Router {
    Router::new().fallback(handle).with_state(MockState { source })
}

async fn handle(State(st): State<MockState>, req: Request) -> Response {
    let method = req.method().as_str().to_string();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let query = uri.query().map(parse_query).unwrap_or_default();
    let headers: BTreeMap<String, String> = req
        .headers()
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|s| (k.as_str().to_string(), s.to_string())))
        .collect();
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX).await.unwrap_or_default();
    let body = (!body_bytes.is_empty()).then(|| String::from_utf8_lossy(&body_bytes).into_owned());

    let mock_req = MockRequest::normalized(&method, &path, query, headers, body);
    let rules = match st.source.active_rules().await {
        Ok(r) => r,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "rule source error").into_response(),
    };
    match match_request(&mock_req, &rules) {
        Some(r) => {
            // 命中后按配置延时(模拟慢响应);0 则不等待。
            if r.response.delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(r.response.delay_ms)).await;
            }
            build_response(&r.response, &mock_req)
        }
        None => (StatusCode::NOT_FOUND, "no mock rule matched").into_response(),
    }
}

/// 渲染响应 body:启用 `template` feature 时按 minijinja 模板注入请求上下文,
/// 渲染失败回落原始串(绝不 500);未启用则原样返回。
#[cfg(feature = "template")]
fn render_body(raw: String, req: &MockRequest) -> String {
    crate::domain::render_body(&raw, req).unwrap_or(raw)
}
#[cfg(not(feature = "template"))]
fn render_body(raw: String, _req: &MockRequest) -> String {
    raw
}

/// `a=1&b=2` → map(简化版:不做 URL 解码,骨架够用,后续可接 form_urlencoded)。
fn parse_query(q: &str) -> BTreeMap<String, String> {
    q.split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| match kv.split_once('=') {
            Some((k, v)) => (k.to_string(), v.to_string()),
            None => (kv.to_string(), String::new()),
        })
        .collect()
}

fn build_response(resp: &MockResponse, req: &MockRequest) -> Response {
    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::OK);
    let mut builder = Response::builder().status(status);
    for (k, v) in &resp.headers {
        builder = builder.header(k, v);
    }
    let rendered = render_body(resp.body.clone().unwrap_or_default(), req);
    builder
        .body(Body::from(Bytes::from(rendered)))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryRuleSource;
    use crate::domain::{MatchRule, MockRule, MockResponse};
    use axum::http::Request as HttpRequest;
    use tower::ServiceExt;

    fn ping_rule() -> MockRule {
        MockRule {
            id: "ping".into(),
            rule: MatchRule { method: Some("GET".into()), path: "/ping".into(), ..Default::default() },
            response: MockResponse {
                status: 200,
                headers: vec![("content-type".into(), "text/plain".into())],
                body: Some("pong".into()),
            },
        }
    }

    fn app() -> Router {
        router(Arc::new(InMemoryRuleSource::new(vec![ping_rule()])))
    }

    async fn body_string(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[tokio::test]
    async fn matched_request_replays_mock_response() {
        let resp = app()
            .oneshot(HttpRequest::builder().method("GET").uri("/ping").body(Body::empty()).expect("req"))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get("content-type").expect("ct"), "text/plain");
        assert_eq!(body_string(resp).await, "pong");
    }

    #[tokio::test]
    async fn unmatched_request_returns_404() {
        let resp = app()
            .oneshot(HttpRequest::builder().method("GET").uri("/nope").body(Body::empty()).expect("req"))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[cfg(feature = "template")]
    #[tokio::test]
    async fn templated_body_renders_request_context() {
        // 响应 body 含模板,命中后用请求(path/query)渲染。
        let rule = MockRule {
            id: "echo".into(),
            rule: MatchRule { method: Some("GET".into()), path: "/echo/*".into(), ..Default::default() },
            response: MockResponse {
                status: 200,
                headers: vec![],
                body: Some(r#"{"path":"{{ path }}","status":"{{ query.status }}"}"#.into()),
            },
        };
        let app = router(Arc::new(InMemoryRuleSource::new(vec![rule])));
        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .method("GET")
                    .uri("/echo/9?status=paid")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_string(resp).await, r#"{"path":"/echo/9","status":"paid"}"#);
    }

    #[tokio::test]
    async fn wrong_method_returns_404() {
        let resp = app()
            .oneshot(HttpRequest::builder().method("POST").uri("/ping").body(Body::empty()).expect("req"))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
