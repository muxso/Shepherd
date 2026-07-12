use std::sync::Arc;
use std::time::Duration;

use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Method, StatusCode};
use axum::routing::get;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
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

pub struct RouteGroup {
    pub label: &'static str,
    pub router: Router,
}

pub fn group(label: &'static str, router: Router) -> RouteGroup {
    RouteGroup { label, router }
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
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024));
    if let Some(cors) = cors_layer() {
        tracing::info!("CORS 允许列表已启用");
        app = app.layer(cors);
    }
    if let Some(rl) = RateLimiter::from_env() {
        tracing::info!("限流已启用(每客户端令牌桶)");
        app = app.layer(axum::middleware::from_fn_with_state(rl, ratelimit::layer));
    }
    // Unified error bodies: normalize plain-text 4xx/5xx into problem+json
    // (including framework 404s and the timeout/rate-limit/body-size layers).
    app = app.layer(axum::middleware::from_fn(crate::problem::normalize));
    // Outermost: record the final status (after timeout/rate-limit/CORS conversions).
    app.layer(axum::middleware::from_fn_with_state(metrics, metrics::track))
}
