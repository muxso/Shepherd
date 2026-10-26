use std::sync::Arc;
use std::time::Duration;

use axum::extract::Request;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::metrics::{self, Metrics};
use crate::ratelimit::{self, RateLimiter};

/// Mounts a CORS allowlist when `SHEPHERD_CORS_ORIGINS` (comma-separated) is set;
/// otherwise no cross-origin access (same-origin, the safe default).
fn cors_layer() -> Option<CorsLayer> {
    let origins = std::env::var("SHEPHERD_CORS_ORIGINS").ok()?;
    let list: Vec<_> = origins.split(',').filter_map(|o| o.trim().parse().ok()).collect();
    if list.is_empty() {
        return None;
    }
    Some(
        CorsLayer::new()
            .allow_origin(list)
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
            .allow_headers([CONTENT_TYPE, AUTHORIZATION])
            .allow_credentials(true),
    )
}

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const LONG_RUN_TIMEOUT: Duration = Duration::from_secs(600);

/// Execution endpoints that run scenarios/plans/cases in-request and can
/// legitimately outlive the default guard. Everything else keeps 30s.
fn timeout_for(method: &Method, path: &str) -> Duration {
    if method != Method::POST {
        return DEFAULT_TIMEOUT;
    }
    let long_run = path == "/api/scenario/batch-run"
        || (path.ends_with("/run")
            && (path.starts_with("/api/scenario/")
                || path.starts_with("/api/case/")
                || path.starts_with("/test-plan/")
                || path.starts_with("/decomposition/")));
    if long_run {
        LONG_RUN_TIMEOUT
    } else {
        DEFAULT_TIMEOUT
    }
}

async fn timeout_by_route(req: Request, next: Next) -> Response {
    let dur = timeout_for(req.method(), req.uri().path());
    match tokio::time::timeout(dur, next.run(req)).await {
        Ok(resp) => resp,
        Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
    }
}

pub struct RouteGroup {
    pub label: &'static str,
    pub router: Router,
}

const DEFAULT_REQUEST_BODY_LIMIT_BYTES: usize = 2 * 1024 * 1024;
/// Project files are sent as base64 JSON. A 50 MiB attachment expands to about
/// 66.7 MiB before the JSON envelope, so leave a small amount of headroom.
pub const PROJECT_FILE_REQUEST_BODY_LIMIT_BYTES: usize = 70 * 1024 * 1024;

pub fn group(label: &'static str, router: Router) -> RouteGroup {
    group_with_body_limit(label, router, DEFAULT_REQUEST_BODY_LIMIT_BYTES)
}

pub fn group_with_body_limit(
    label: &'static str,
    router: Router,
    max_body_bytes: usize,
) -> RouteGroup {
    RouteGroup {
        label,
        router: router
            .layer(axum::extract::DefaultBodyLimit::max(max_body_bytes))
            .layer(RequestBodyLimitLayer::new(max_body_bytes)),
    }
}

pub fn assemble(groups: Vec<RouteGroup>) -> Router {
    let metrics = Arc::new(Metrics::default());
    let mut app = Router::new();
    for g in groups {
        tracing::info!(domain = g.label, "mounted route group");
        app = app.merge(g.router);
    }
    let metrics_route =
        Router::new().route("/metrics", get(metrics::handler)).with_state(metrics.clone());
    let mut app = app
        .merge(metrics_route)
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(timeout_by_route));
    if let Some(cors) = cors_layer() {
        tracing::info!("CORS allowlist enabled");
        app = app.layer(cors);
    }
    if let Some(rl) = RateLimiter::from_env() {
        tracing::info!("rate limiting enabled (per-client token bucket)");
        app = app.layer(axum::middleware::from_fn_with_state(rl, ratelimit::layer));
    }
    // Unified error bodies: normalize plain-text 4xx/5xx into problem+json
    // (including framework 404s and the timeout/rate-limit/body-size layers).
    app = app.layer(axum::middleware::from_fn(crate::problem::normalize));
    // Outermost: record the final status (after timeout/rate-limit/CORS conversions).
    app.layer(axum::middleware::from_fn_with_state(metrics, metrics::track))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, Bytes};
    use axum::http::Request;
    use axum::http::StatusCode;
    use axum::routing::post;
    use tower::ServiceExt;

    #[test]
    fn run_endpoints_get_long_timeout() {
        for path in [
            "/api/scenario/abc-123/run",
            "/api/scenario/batch-run",
            "/api/case/abc-123/run",
            "/test-plan/abc-123/run",
            "/test-plan/abc-123/cases/c-1/run",
            "/decomposition/abc-123/run",
        ] {
            assert_eq!(timeout_for(&Method::POST, path), LONG_RUN_TIMEOUT, "{path}");
        }
    }

    #[test]
    fn other_routes_keep_default_timeout() {
        for path in [
            "/api/scenario",
            "/api/scenario/abc-123",
            "/api/import-schedule/abc-123/run",
            "/perf/run",
            "/perf/scenario/run",
            "/test-plan/abc-123",
            "/api/user/login",
        ] {
            assert_eq!(timeout_for(&Method::POST, path), DEFAULT_TIMEOUT, "{path}");
        }
        // Non-POST never gets the long-run exemption.
        assert_eq!(timeout_for(&Method::GET, "/api/scenario/abc-123/run"), DEFAULT_TIMEOUT);
    }

    #[tokio::test]
    async fn project_file_group_accepts_large_workbook_payloads_only_on_its_routes() {
        let files = Router::new()
            .route("/api/project-file", post(|_body: Bytes| async { StatusCode::CREATED }));
        let ordinary =
            Router::new().route("/ordinary", post(|_body: Bytes| async { StatusCode::CREATED }));
        let app = assemble(vec![
            group_with_body_limit("project-file", files, PROJECT_FILE_REQUEST_BODY_LIMIT_BYTES),
            group("ordinary", ordinary),
        ]);
        let payload = vec![b'x'; 3 * 1024 * 1024];

        let file_response = app
            .clone()
            .oneshot(Request::post("/api/project-file").body(Body::from(payload.clone())).unwrap())
            .await
            .unwrap();
        assert_eq!(file_response.status(), StatusCode::CREATED);

        let ordinary_response = app
            .oneshot(Request::post("/ordinary").body(Body::from(payload)).unwrap())
            .await
            .unwrap();
        assert_eq!(ordinary_response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
