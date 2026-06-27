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

/// `SHEPHERD_CORS_ORIGINS`(逗号分隔)设置时挂 CORS 允许列表;未设置则不跨域(同源,安全默认)。
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
    let metrics_route = Router::new().route("/metrics", get(metrics::handler)).with_state(metrics.clone());
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
    // 统一错误体:把纯文本 4xx/5xx 归一成 problem+json(含框架 404、超时/限流/体积层)。
    app = app.layer(axum::middleware::from_fn(crate::problem::normalize));
    // 最外层:记录最终状态(含超时/限流/CORS 转换)。
    app.layer(axum::middleware::from_fn_with_state(metrics, metrics::track))
}
