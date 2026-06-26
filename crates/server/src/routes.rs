use std::time::Duration;

use axum::http::StatusCode;
use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

pub struct RouteGroup {
    pub label: &'static str,
    pub router: Router,
}

pub fn group(label: &'static str, router: Router) -> RouteGroup {
    RouteGroup { label, router }
}

pub fn assemble(groups: Vec<RouteGroup>) -> Router {
    let mut app = Router::new();
    for g in groups {
        tracing::info!(domain = g.label, "mounted route group");
        app = app.merge(g.router);
    }
    app.layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024))
}
