//! 路由注册表:把各业务模块的子路由按**领域**分组合并成单一 app,并统一挂载全局中间件。
//!
//! 取代原先 main 里 38 连串 `.merge(...)` + 尾部三个 `.layer(...)`。收益:
//! - **降合并冲突面**:新增路由 append 到所属领域分组,不同领域改不同行,而非都挤在同一条尾链。
//! - **中间件策略单点**:链路追踪 / 30s 超时 / 2MiB 体积上限集中此处,改一处即全局生效、可复用。
//! - **启动可观测**:boot 时按领域打印挂载日志。
//!
//! 合并语义与原链一致(axum `merge` 要求路径不重叠;原链能跑即各子路由路径互不冲突,
//! 故分组/重排安全)。各子路由的状态在各自 `router()` 内已注入,这里只做无状态合并 + 加层。

use std::time::Duration;

use axum::http::StatusCode;
use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

/// 一个领域分组:`label` 仅用于启动日志/可读性,运行时 axum 合并后已扁平化、不影响路由匹配。
pub struct RouteGroup {
    pub label: &'static str,
    pub router: Router,
}

/// 构造一个领域分组。
pub fn group(label: &'static str, router: Router) -> RouteGroup {
    RouteGroup { label, router }
}

/// 把各领域分组合并成单一 app,并由外到内挂载全局中间件:
/// 请求日志 → 整体 30s 超时 → 2MiB 请求体上限(防超大 body 打爆内存)。
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
