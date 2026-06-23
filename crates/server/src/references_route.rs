//! 组装根桥:`GET /api/definition/{id}/references` —— 接口定义的「引用关系」反查。
//!
//! 跨两聚合:接口用例(api-definition 拥有 ms_api_case)与场景步骤(api-scenario 拥有
//! ms_api_scenario_step)。按约定组装根不碰裸 sqlx —— 仅编排两个 crate 的仓储:
//!   1) apidef_repo.list_cases(id) → 该定义下的用例;
//!   2) scenario_repo.list_scenarios_referencing_cases(case_ids) → 引用了这些用例的场景(去重)。
//! 读端点,开放(与接口定义其它读端点一致)。

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};

use api_definition::ports::ApiDefinitionRepository;
use api_scenario::ports::ApiScenarioRepository;

#[derive(Clone)]
struct ReferencesState {
    apidef: Arc<dyn ApiDefinitionRepository>,
    scenario: Arc<dyn ApiScenarioRepository>,
}

pub fn router(
    apidef: Arc<dyn ApiDefinitionRepository>,
    scenario: Arc<dyn ApiScenarioRepository>,
) -> Router {
    Router::new()
        .route("/api/definition/{id}/references", get(list_references))
        .with_state(ReferencesState { apidef, scenario })
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RefItem {
    id: String,
    name: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReferencesResponse {
    /// 该定义下的接口用例。
    cases: Vec<RefItem>,
    /// 引用了上述任一用例的场景(去重)。
    scenarios: Vec<RefItem>,
}

#[utoipa::path(
    get, path = "/api/definition/{id}/references", tag = "api-definition",
    params(("id" = String, Path)),
    responses((status = 200, body = ReferencesResponse), (status = 500))
)]
async fn list_references(State(st): State<ReferencesState>, Path(id): Path<String>) -> Response {
    let cases = match st.apidef.list_cases(&id).await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    };
    let case_ids: Vec<String> = cases.iter().map(|c| c.id.clone()).collect();
    let scenarios = match st.scenario.list_scenarios_referencing_cases(&case_ids).await {
        Ok(s) => s,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    };
    let body = ReferencesResponse {
        cases: cases.into_iter().map(|c| RefItem { id: c.id, name: c.name }).collect(),
        scenarios: scenarios.into_iter().map(|s| RefItem { id: s.id, name: s.name }).collect(),
    };
    (StatusCode::OK, Json(body)).into_response()
}

#[derive(OpenApi)]
#[openapi(
    paths(list_references),
    components(schemas(ReferencesResponse, RefItem)),
    tags((name = "api-definition", description = "接口定义"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
