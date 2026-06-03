//! OpenAPI 文档聚合 + Swagger UI。
//! 各 http 适配器各自 `#[derive(OpenApi)]` 导出 `openapi()`,这里合并成一份总文档,
//! 注入 bearer 安全方案,挂 `/api-docs/openapi.json` 与 `/swagger-ui`(CDN 资产,无需打包)。

use axum::response::Html;
use axum::routing::get;
use axum::{Json, Router};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

/// 总文档骨架:标题/版本 + bearer 安全方案。
#[derive(OpenApi)]
#[openapi(
    info(title = "Shepherd API", version = env!("CARGO_PKG_VERSION"), description = "AI 研发监督平台 REST API"),
    modifiers(&BearerAddon),
    tags((name = "auth", description = "鉴权"))
)]
struct Base;

struct BearerAddon;
impl Modify for BearerAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let comp = openapi.components.get_or_insert_with(Default::default);
        comp.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new().scheme(HttpAuthScheme::Bearer).bearer_format("opaque").build(),
            ),
        );
    }
}

/// 合并所有上下文的 OpenAPI(逐 crate 接入;未注解的暂不并入)。
pub fn merged() -> utoipa::openapi::OpenApi {
    let mut doc = Base::openapi();
    doc.merge(requirement::adapters::http::openapi());
    doc.merge(task::adapters::http::openapi());
    doc.merge(delivery::adapters::http::openapi());
    doc.merge(verification::adapters::http::openapi());
    doc.merge(skill::adapters::http::openapi());
    doc
}

async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(merged())
}

async fn swagger_ui() -> Html<&'static str> {
    Html(SWAGGER_HTML)
}

pub fn routes() -> Router {
    Router::new()
        .route("/api-docs/openapi.json", get(openapi_json))
        .route("/swagger-ui", get(swagger_ui))
}

const SWAGGER_HTML: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"/><title>Shepherd API</title>
<link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css"/></head>
<body><div id="swagger-ui"></div>
<script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
<script>window.onload=()=>{window.ui=SwaggerUIBundle({url:'/api-docs/openapi.json',dom_id:'#swagger-ui'})}</script>
</body></html>"#;
