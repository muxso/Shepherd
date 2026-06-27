use axum::response::Html;
use axum::routing::get;
use axum::{Json, Router};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

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

pub fn merged() -> utoipa::openapi::OpenApi {
    let mut doc = Base::openapi();
    doc.merge(requirement::adapters::http::openapi());
    doc.merge(task::adapters::http::openapi());
    doc.merge(delivery::adapters::http::openapi());
    doc.merge(verification::adapters::http::openapi());
    doc.merge(skill::adapters::http::openapi());
    doc.merge(system_setting::adapters::http::openapi());
    doc.merge(project::adapters::http::openapi());
    doc.merge(project::adapters::member_http::openapi());
    doc.merge(case::adapters::http::openapi());
    doc.merge(bug::adapters::http::openapi());
    doc.merge(comment::adapters::http::openapi());
    doc.merge(follow::adapters::http::openapi());
    doc.merge(test_plan::adapters::http::openapi());
    doc.merge(api_test::adapters::http::openapi());
    doc.merge(api_definition::adapters::http::openapi());
    doc.merge(api_scenario::adapters::http::openapi());
    doc.merge(environment::adapters::http::openapi());
    doc.merge(case_management::adapters::http::openapi());
    doc.merge(runner::adapters::http::openapi());
    doc.merge(crate::scenario_run::openapi());
    doc.merge(crate::perf_run::openapi());
    doc.merge(crate::plan_run::openapi());
    doc.merge(crate::decomposition_run::openapi());
    doc.merge(crate::references_route::openapi());
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
