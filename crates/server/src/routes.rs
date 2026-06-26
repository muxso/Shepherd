use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::metrics::{self, Metrics};

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
    let metrics_route = Router::new().route("/metrics", get(metrics::handler)).with_state(metrics.clone());
    app.merge(metrics_route)
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024))
        // 最外层:记录最终状态(含超时/限流转换)。
        .layer(axum::middleware::from_fn_with_state(metrics, metrics::track))
}
