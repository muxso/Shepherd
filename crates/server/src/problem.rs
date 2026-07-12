//! Unified error bodies: normalizes plain-text error responses into RFC 7807
//! `application/problem+json`. As one of the outermost response middlewares it
//! covers handlers' `(StatusCode, &str)` errors, framework 404s, and the 4xx/5xx
//! produced by the timeout/rate-limit/body-size layers — no per-handler rework.
//! Error bodies that are already JSON pass through; the original status code and
//! headers (e.g. rate limiting's `Retry-After`) are preserved.

use axum::body::Body;
use axum::extract::Request;
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use serde_json::json;

const PROBLEM_JSON: &str = "application/problem+json";

fn problem_body(status: StatusCode, detail: &str) -> Vec<u8> {
    let title = status.canonical_reason().unwrap_or("Error");
    let detail = if detail.is_empty() { title } else { detail };
    serde_json::to_vec(&json!({
        "type": "about:blank",
        "title": title,
        "status": status.as_u16(),
        "detail": detail,
    }))
    .unwrap_or_default()
}

pub async fn normalize(req: Request, next: Next) -> Response {
    let resp = next.run(req).await;
    let status = resp.status();
    if !(status.is_client_error() || status.is_server_error()) {
        return resp;
    }
    let already_json = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.contains("json"));
    if already_json {
        return resp;
    }
    let (mut parts, body) = resp.into_parts();
    let bytes = axum::body::to_bytes(body, 64 * 1024).await.unwrap_or_default();
    let detail = String::from_utf8_lossy(&bytes);
    let out = problem_body(parts.status, detail.trim());
    parts.headers.insert(CONTENT_TYPE, HeaderValue::from_static(PROBLEM_JSON));
    parts.headers.remove(CONTENT_LENGTH); // body replaced, length is stale; axum recomputes it
    Response::from_parts(parts, Body::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::RETRY_AFTER;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn get_req(path: &str) -> Request {
        Request::builder().uri(path).body(Body::empty()).unwrap()
    }

    fn ctype(r: &Response) -> String {
        r.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string()
    }

    async fn json_body(r: Response) -> serde_json::Value {
        let b = axum::body::to_bytes(r.into_body(), 64 * 1024).await.unwrap();
        serde_json::from_slice(&b).unwrap()
    }

    fn app() -> Router {
        Router::new()
            .route("/ok", get(|| async { "fine" }))
            .route("/bad", get(|| async { (StatusCode::BAD_REQUEST, "nope").into_response() }))
            .route(
                "/limited",
                get(|| async {
                    let mut r = (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
                    r.headers_mut().insert(RETRY_AFTER, HeaderValue::from(7));
                    r
                }),
            )
            .route(
                "/json-err",
                get(|| async {
                    (StatusCode::CONFLICT, axum::Json(json!({"code": "dup"}))).into_response()
                }),
            )
            .layer(axum::middleware::from_fn(normalize))
    }

    #[tokio::test]
    async fn unmatched_route_becomes_problem_json_404() {
        let r = app().oneshot(get_req("/missing")).await.unwrap();
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        assert_eq!(ctype(&r), PROBLEM_JSON);
        let v = json_body(r).await;
        assert_eq!(v["status"], 404);
        assert_eq!(v["title"], "Not Found");
    }

    #[tokio::test]
    async fn plaintext_handler_error_becomes_problem_json_with_detail() {
        let r = app().oneshot(get_req("/bad")).await.unwrap();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        assert_eq!(ctype(&r), PROBLEM_JSON);
        let v = json_body(r).await;
        assert_eq!(v["status"], 400);
        assert_eq!(v["detail"], "nope");
    }

    #[tokio::test]
    async fn preserves_status_and_headers_like_retry_after() {
        let r = app().oneshot(get_req("/limited")).await.unwrap();
        assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(ctype(&r), PROBLEM_JSON);
        assert_eq!(r.headers().get(RETRY_AFTER).unwrap(), "7");
    }

    #[tokio::test]
    async fn json_error_passes_through_untouched() {
        let r = app().oneshot(get_req("/json-err")).await.unwrap();
        assert_eq!(r.status(), StatusCode::CONFLICT);
        assert!(ctype(&r).contains("application/json"));
        let v = json_body(r).await;
        assert_eq!(v["code"], "dup");
    }

    #[tokio::test]
    async fn success_passes_through_untouched() {
        let r = app().oneshot(get_req("/ok")).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert_ne!(ctype(&r), PROBLEM_JSON);
    }
}
