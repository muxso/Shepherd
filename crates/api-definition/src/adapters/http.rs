//! 接口定义上下文的 HTTP 适配器。
//!
//! 路由覆盖接口定义 / 用例 / Mock 三聚合的增查。本层只做 DTO 翻译 + 错误码映射,
//! 校验规则全在 domain/application。RBAC 资源串为 `API_DEFINITION`:写端点需
//! `API_DEFINITION:ADD`(无令牌→401,缺权限→403),读端点开放。
//! 错误映射:校验失败→400,接口定义不存在→404,仓储错误→500。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{post, put},
    Json, Router,
};
use crate::application::{
    AddApiCaseError, AddApiCaseUseCase, AddApiMockError, AddApiMockUseCase, CreateApiCaseError,
    CreateApiCaseUseCase, CreateApiDefinitionError, CreateApiDefinitionUseCase,
    ImportApiDefinitionsUseCase, ImportError, ListApiCasesUseCase, ListApiDefinitionsUseCase,
    ListApiMocksUseCase, ListProjectCasesUseCase,
};
use kernel::page::PageRequest;
use crate::domain::{ApiCase, ApiDefinition, ApiModule, ApiMock, ApiProtocol, NewApiModule};
use crate::ports::ApiDefinitionRepository;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct ApiDefinitionState {
    create_def: CreateApiDefinitionUseCase,
    list_def: ListApiDefinitionsUseCase,
    add_case: AddApiCaseUseCase,
    list_case: ListApiCasesUseCase,
    create_case: CreateApiCaseUseCase,
    list_project_case: ListProjectCasesUseCase,
    add_mock: AddApiMockUseCase,
    list_mock: ListApiMocksUseCase,
    import_def: ImportApiDefinitionsUseCase,
    repo: Arc<dyn ApiDefinitionRepository>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<ApiDefinitionState> for Arc<dyn SessionStore> {
    fn from_ref(s: &ApiDefinitionState) -> Self {
        s.sessions.clone()
    }
}

/// 装配接口定义上下文的全部路由。内部从仓储构建各用例。
pub fn router(
    repo: Arc<dyn ApiDefinitionRepository>,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    let state = ApiDefinitionState {
        create_def: CreateApiDefinitionUseCase::new(repo.clone()),
        list_def: ListApiDefinitionsUseCase::new(repo.clone()),
        add_case: AddApiCaseUseCase::new(repo.clone()),
        list_case: ListApiCasesUseCase::new(repo.clone()),
        create_case: CreateApiCaseUseCase::new(repo.clone()),
        list_project_case: ListProjectCasesUseCase::new(repo.clone()),
        add_mock: AddApiMockUseCase::new(repo.clone()),
        list_mock: ListApiMocksUseCase::new(repo.clone()),
        import_def: ImportApiDefinitionsUseCase::new(repo.clone()),
        repo,
        sessions,
    };
    Router::new()
        .route("/api/definition", post(create_definition).get(list_definitions))
        .route("/api/definition/import", post(import_definitions))
        .route("/api/definition/{id}", axum::routing::get(get_definition))
        .route("/api/definition/{id}/spec", put(update_definition_spec))
        .route("/api/definition/{id}/changes", axum::routing::get(list_definition_changes))
        .route("/api/definition/{id}/status", put(update_definition_status))
        .route("/api/definition/{id}/case", post(create_case).get(list_cases))
        .route("/api/case", post(create_standalone_case).get(list_project_cases))
        .route("/api/definition/{id}/mock", post(create_mock).get(list_mocks))
        .route("/api/module", post(create_module).get(list_modules))
        .route("/api/module/{id}", put(rename_module).delete(delete_module))
        .route("/api/definition/{id}/module", put(move_definition))
        .route("/api/task-case", post(link_task_case).get(list_task_cases))
        .route("/api/task-case/unlink", post(unlink_task_case))
        .with_state(state)
}

// ---------- DTO ----------

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ApiDefinitionResponse {
    id: String,
    project_id: String,
    name: String,
    protocol: String,
    method: String,
    path: String,
    status: String,
    module_id: Option<String>,
    /// 请求/响应规格(不透明 JSON;约定见 0037 迁移)。
    spec: serde_json::Value,
}

impl From<ApiDefinition> for ApiDefinitionResponse {
    fn from(d: ApiDefinition) -> Self {
        Self {
            id: d.id,
            project_id: d.project_id,
            name: d.name,
            protocol: d.protocol.as_str().to_string(),
            method: d.method,
            path: d.path,
            status: d.status.as_str().to_string(),
            module_id: d.module_id,
            // spec 以 TEXT 存 JSON 文本;无法解析时回退为 {}。
            spec: serde_json::from_str(&d.spec).unwrap_or_else(|_| serde_json::json!({})),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ApiModuleResponse {
    id: String,
    project_id: String,
    parent_id: Option<String>,
    name: String,
}

impl From<ApiModule> for ApiModuleResponse {
    fn from(m: ApiModule) -> Self {
        Self { id: m.id, project_id: m.project_id, parent_id: m.parent_id, name: m.name }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ModuleCreateBody {
    project_id: String,
    #[serde(default)]
    parent_id: Option<String>,
    name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ModuleRenameBody {
    name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct MoveDefinitionBody {
    /// 目标模块 id;null/缺省 = 移出到未归类。
    #[serde(default)]
    module_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ApiDefinitionCreateBody {
    project_id: String,
    name: String,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct DefinitionListQuery {
    project_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ApiCaseResponse {
    id: String,
    api_definition_id: String,
    project_id: String,
    name: String,
    method: String,
    url: String,
    body: Option<String>,
    assertions: serde_json::Value,
    processors: serde_json::Value,
}

impl From<ApiCase> for ApiCaseResponse {
    fn from(c: ApiCase) -> Self {
        Self {
            id: c.id,
            api_definition_id: c.api_definition_id,
            project_id: c.project_id,
            name: c.name,
            method: c.method,
            url: c.url,
            body: c.body,
            assertions: c.assertions,
            processors: c.processors,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ApiCaseCreateBody {
    name: String,
    method: String,
    url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assertions: Option<serde_json::Value>,
    /// 前后置处理器(EXTRACT/WAIT)数组;缺省空。
    #[serde(default)]
    processors: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ApiMockResponse {
    id: String,
    api_definition_id: String,
    name: String,
    match_rule: serde_json::Value,
    response_status: i32,
    response_body: Option<String>,
    enabled: bool,
}

impl From<ApiMock> for ApiMockResponse {
    fn from(m: ApiMock) -> Self {
        Self {
            id: m.id,
            api_definition_id: m.api_definition_id,
            name: m.name,
            match_rule: m.match_rule,
            response_status: m.response_status,
            response_body: m.response_body,
            enabled: m.enabled,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ApiMockCreateBody {
    name: String,
    #[serde(default)]
    match_rule: Option<serde_json::Value>,
    #[serde(default)]
    response_status: Option<i32>,
    #[serde(default)]
    response_body: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

// ---------- 接口定义 handlers ----------

#[utoipa::path(post, path = "/api/definition", tag = "api-definition", request_body = ApiDefinitionCreateBody, responses((status = 201, body = ApiDefinitionResponse), (status = 400)), security(("bearer" = [])))]
async fn create_definition(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Json(req): Json<ApiDefinitionCreateBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    // 协议缺省 HTTP;非法协议串直接 400(不落到用例)。
    let protocol = match req.protocol.as_deref() {
        None => ApiProtocol::Http,
        Some(s) => match ApiProtocol::parse(s) {
            Some(p) => p,
            None => return (StatusCode::BAD_REQUEST, "unknown protocol").into_response(),
        },
    };
    let method = req.method.as_deref().unwrap_or_default();
    let path = req.path.as_deref().unwrap_or_default();
    match st.create_def.execute(&req.project_id, &req.name, protocol, method, path).await {
        Ok(d) => {
            // 审计:记录创建(best-effort,失败不阻断)。
            let _ = st
                .repo
                .record_definition_change(&d.id, "CREATE", &format!("{} {}", d.method, d.path), &user.user_id)
                .await;
            (StatusCode::CREATED, Json(ApiDefinitionResponse::from(d))).into_response()
        }
        Err(CreateApiDefinitionError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid api definition payload").into_response()
        }
        Err(CreateApiDefinitionError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/api/definition", tag = "api-definition", params(DefinitionListQuery), responses((status = 200, body = [ApiDefinitionResponse])))]
async fn list_definitions(
    State(st): State<ApiDefinitionState>,
    Query(q): Query<DefinitionListQuery>,
) -> Response {
    match st.list_def.execute(&q.project_id).await {
        Ok(list) => {
            let items: Vec<ApiDefinitionResponse> =
                list.into_iter().map(ApiDefinitionResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(get, path = "/api/definition/{id}", tag = "api-definition", params(("id" = String, Path)), responses((status = 200, body = ApiDefinitionResponse), (status = 404)))]
async fn get_definition(
    State(st): State<ApiDefinitionState>,
    Path(id): Path<String>,
) -> Response {
    // 单条按 id 读取直查仓储(读端点,无需用例编排)。
    match st.repo.get_definition(&id).await {
        Ok(Some(d)) => (StatusCode::OK, Json(ApiDefinitionResponse::from(d))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "api definition not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct SpecUpdateBody {
    /// 完整请求/响应规格(JSON 对象);服务端不透明存取。
    spec: serde_json::Value,
}

#[utoipa::path(put, path = "/api/definition/{id}/spec", tag = "api-definition", params(("id" = String, Path)), request_body = SpecUpdateBody, responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn update_definition_spec(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Path(id): Path<String>,
    Json(req): Json<SpecUpdateBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    // 先确认存在,避免对幽灵 id 静默写入。
    match st.repo.get_definition(&id).await {
        Ok(Some(_)) => {}
        Ok(None) => return (StatusCode::NOT_FOUND, "api definition not found").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
    let spec = req.spec.to_string();
    match st.repo.update_definition_spec(&id, &spec).await {
        Ok(()) => {
            let _ = st
                .repo
                .record_definition_change(&id, "UPDATE_SPEC", "更新请求/响应规格", &user.user_id)
                .await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct StatusUpdateBody {
    /// DRAFT | DEBUGGING | COMPLETED | DEPRECATED
    status: String,
}

#[utoipa::path(put, path = "/api/definition/{id}/status", tag = "api-definition", params(("id" = String, Path)), request_body = StatusUpdateBody, responses((status = 204), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn update_definition_status(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Path(id): Path<String>,
    Json(req): Json<StatusUpdateBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    // 校验状态值合法。
    let Some(status) = crate::domain::ApiStatus::parse(&req.status) else {
        return (StatusCode::BAD_REQUEST, "invalid status").into_response();
    };
    match st.repo.get_definition(&id).await {
        Ok(Some(_)) => {}
        Ok(None) => return (StatusCode::NOT_FOUND, "api definition not found").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
    match st.repo.update_definition_status(&id, status.as_str()).await {
        Ok(()) => {
            let _ = st
                .repo
                .record_definition_change(&id, "STATUS", &format!("状态 → {}", status.as_str()), &user.user_id)
                .await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

// ---------- 用例 handlers ----------

#[utoipa::path(post, path = "/api/definition/{id}/case", tag = "api-definition", params(("id" = String, Path)), request_body = ApiCaseCreateBody, responses((status = 201, body = ApiCaseResponse), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn create_case(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Path(id): Path<String>,
    Json(req): Json<ApiCaseCreateBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let assertions = req.assertions.unwrap_or_else(|| serde_json::json!([]));
    let processors = req.processors.unwrap_or_else(|| serde_json::json!([]));
    match st.add_case.execute(&id, &req.name, &req.method, &req.url, req.body, assertions, processors).await {
        Ok(c) => (StatusCode::CREATED, Json(ApiCaseResponse::from(c))).into_response(),
        Err(AddApiCaseError::NotFound) => {
            (StatusCode::NOT_FOUND, "api definition not found").into_response()
        }
        Err(AddApiCaseError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid api case payload").into_response()
        }
        Err(AddApiCaseError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/api/definition/{id}/case", tag = "api-definition", params(("id" = String, Path)), responses((status = 200, body = [ApiCaseResponse])))]
async fn list_cases(State(st): State<ApiDefinitionState>, Path(id): Path<String>) -> Response {
    match st.list_case.execute(&id).await {
        Ok(list) => {
            let items: Vec<ApiCaseResponse> =
                list.into_iter().map(ApiCaseResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

// ---------- 项目级(独立)用例 handlers ----------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct StandaloneCaseBody {
    project_id: String,
    #[serde(default)]
    api_definition_id: Option<String>,
    name: String,
    method: String,
    url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assertions: Option<serde_json::Value>,
    /// 前后置处理器(EXTRACT/WAIT)数组;缺省空。
    #[serde(default)]
    processors: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct CaseListQuery {
    project_id: String,
    #[serde(default = "default_current")]
    current: u32,
    #[serde(default = "default_page_size")]
    page_size: u32,
}

fn default_current() -> u32 {
    1
}
fn default_page_size() -> u32 {
    10
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ApiCasePageResponse {
    total: u64,
    current: u32,
    page_size: u32,
    total_pages: u64,
    items: Vec<ApiCaseResponse>,
}

#[utoipa::path(post, path = "/api/case", tag = "api-definition", request_body = StandaloneCaseBody, responses((status = 201, body = ApiCaseResponse), (status = 400), (status = 401), (status = 403)), security(("bearer" = [])))]
async fn create_standalone_case(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Json(req): Json<StandaloneCaseBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let assertions = req.assertions.unwrap_or_else(|| serde_json::json!([]));
    let processors = req.processors.unwrap_or_else(|| serde_json::json!([]));
    match st
        .create_case
        .execute(
            &req.project_id,
            req.api_definition_id.as_deref(),
            &req.name,
            &req.method,
            &req.url,
            req.body,
            assertions,
            processors,
        )
        .await
    {
        Ok(c) => (StatusCode::CREATED, Json(ApiCaseResponse::from(c))).into_response(),
        Err(CreateApiCaseError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid api case payload").into_response()
        }
        Err(CreateApiCaseError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/api/case", tag = "api-definition", params(CaseListQuery), responses((status = 200, body = ApiCasePageResponse), (status = 400)))]
async fn list_project_cases(
    State(st): State<ApiDefinitionState>,
    Query(q): Query<CaseListQuery>,
) -> Response {
    let page = match PageRequest::new(q.current, q.page_size) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid page params").into_response(),
    };
    match st.list_project_case.execute(&q.project_id, page).await {
        Ok(page) => {
            let body = ApiCasePageResponse {
                total: page.total,
                current: page.current,
                page_size: page.page_size,
                total_pages: page.total_pages(),
                items: page.items.into_iter().map(ApiCaseResponse::from).collect(),
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

// ---------- Mock handlers ----------

#[utoipa::path(post, path = "/api/definition/{id}/mock", tag = "api-definition", params(("id" = String, Path)), request_body = ApiMockCreateBody, responses((status = 201, body = ApiMockResponse), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn create_mock(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Path(id): Path<String>,
    Json(req): Json<ApiMockCreateBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let match_rule = req.match_rule.unwrap_or_else(|| serde_json::json!({}));
    let response_status = req.response_status.unwrap_or(200);
    let enabled = req.enabled.unwrap_or(true);
    match st
        .add_mock
        .execute(&id, &req.name, match_rule, response_status, req.response_body, enabled)
        .await
    {
        Ok(m) => (StatusCode::CREATED, Json(ApiMockResponse::from(m))).into_response(),
        Err(AddApiMockError::NotFound) => {
            (StatusCode::NOT_FOUND, "api definition not found").into_response()
        }
        Err(AddApiMockError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid api mock payload").into_response()
        }
        Err(AddApiMockError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/api/definition/{id}/mock", tag = "api-definition", params(("id" = String, Path)), responses((status = 200, body = [ApiMockResponse])))]
async fn list_mocks(State(st): State<ApiDefinitionState>, Path(id): Path<String>) -> Response {
    match st.list_mock.execute(&id).await {
        Ok(list) => {
            let items: Vec<ApiMockResponse> =
                list.into_iter().map(ApiMockResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

// ---------- 模块(文件夹)handlers ----------

async fn create_module(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Json(req): Json<ModuleCreateBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let new = match NewApiModule::new(&req.project_id, req.parent_id.as_deref(), &req.name) {
        Ok(m) => m,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid module payload").into_response(),
    };
    match st.repo.insert_module(&new).await {
        Ok(m) => (StatusCode::CREATED, Json(ApiModuleResponse::from(m))).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

async fn list_modules(
    State(st): State<ApiDefinitionState>,
    Query(q): Query<DefinitionListQuery>,
) -> Response {
    match st.repo.list_modules(&q.project_id).await {
        Ok(list) => {
            let items: Vec<ApiModuleResponse> =
                list.into_iter().map(ApiModuleResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

async fn rename_module(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Path(id): Path<String>,
    Json(req): Json<ModuleRenameBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let name = req.name.trim();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty name").into_response();
    }
    match st.repo.rename_module(&id, name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

async fn delete_module(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.repo.delete_module(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

async fn move_definition(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Path(id): Path<String>,
    Json(req): Json<MoveDefinitionBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let module = req.module_id.as_deref().filter(|s| !s.is_empty());
    match st.repo.set_definition_module(&id, module).await {
        Ok(()) => {
            let detail = match module {
                Some(m) => format!("移入模块 {m}"),
                None => "移出到未归类".to_string(),
            };
            let _ = st.repo.record_definition_change(&id, "MOVE_MODULE", &detail, &user.user_id).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

// ---------- 变更历史(审计)handler ----------

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ApiDefinitionChangeResponse {
    id: String,
    definition_id: String,
    action: String,
    detail: String,
    actor: String,
    created_at: String,
}

impl From<crate::domain::ApiDefinitionChange> for ApiDefinitionChangeResponse {
    fn from(c: crate::domain::ApiDefinitionChange) -> Self {
        Self {
            id: c.id,
            definition_id: c.definition_id,
            action: c.action,
            detail: c.detail,
            actor: c.actor,
            created_at: c.created_at,
        }
    }
}

#[utoipa::path(get, path = "/api/definition/{id}/changes", tag = "api-definition", params(("id" = String, Path)), responses((status = 200, body = [ApiDefinitionChangeResponse])))]
async fn list_definition_changes(State(st): State<ApiDefinitionState>, Path(id): Path<String>) -> Response {
    match st.repo.list_definition_changes(&id).await {
        Ok(list) => {
            let items: Vec<ApiDefinitionChangeResponse> =
                list.into_iter().map(ApiDefinitionChangeResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

// ---------- 任务 ↔ 用例 关联 handlers ----------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct TaskCaseBody {
    decomposition_id: String,
    task_id: String,
    case_id: String,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct TaskCaseQuery {
    decomposition_id: String,
    task_id: String,
}

async fn link_task_case(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Json(b): Json<TaskCaseBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.repo.link_task_case(&b.decomposition_id, &b.task_id, &b.case_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

async fn unlink_task_case(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Json(b): Json<TaskCaseBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.repo.unlink_task_case(&b.decomposition_id, &b.task_id, &b.case_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

async fn list_task_cases(
    State(st): State<ApiDefinitionState>,
    Query(q): Query<TaskCaseQuery>,
) -> Response {
    match st.repo.list_cases_for_task(&q.decomposition_id, &q.task_id).await {
        Ok(list) => {
            let items: Vec<ApiCaseResponse> = list.into_iter().map(ApiCaseResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

// ---------- 导入 handler ----------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ImportBody {
    project_id: String,
    /// OpenAPI 3.x / Swagger 2.0 文档(JSON)。
    content: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ImportResultResponse {
    created: Vec<ApiDefinitionResponse>,
    skipped: usize,
}

#[utoipa::path(
    post,
    path = "/api/definition/import",
    tag = "api-definition",
    request_body = ImportBody,
    responses((status = 201, body = ImportResultResponse), (status = 400)),
    security(("bearer" = []))
)]
async fn import_definitions(
    user: AuthUser,
    State(st): State<ApiDefinitionState>,
    Json(req): Json<ImportBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.import_def.execute(&req.project_id, &req.content).await {
        Ok(out) => {
            let created: Vec<ApiDefinitionResponse> =
                out.created.into_iter().map(ApiDefinitionResponse::from).collect();
            (StatusCode::CREATED, Json(ImportResultResponse { created, skipped: out.skipped }))
                .into_response()
        }
        Err(ImportError::Parse(_)) => {
            (StatusCode::BAD_REQUEST, "unrecognized openapi/swagger document").into_response()
        }
        Err(ImportError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        create_definition,
        import_definitions,
        list_definitions,
        get_definition,
        update_definition_spec,
        update_definition_status,
        list_definition_changes,
        create_case,
        list_cases,
        create_standalone_case,
        list_project_cases,
        create_mock,
        list_mocks
    ),
    components(schemas(
        ApiDefinitionCreateBody,
        ApiDefinitionResponse,
        SpecUpdateBody,
        StatusUpdateBody,
        ApiDefinitionChangeResponse,
        ApiCaseCreateBody,
        ApiCaseResponse,
        StandaloneCaseBody,
        ApiCasePageResponse,
        ApiMockCreateBody,
        ApiMockResponse,
        ImportBody,
        ImportResultResponse
    )),
    tags((name = "api-definition", description = "接口定义"))
)]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use crate::adapters::InMemoryApiDefinitionRepository;
    use kernel::permission::PermissionSet;
    use std::sync::Arc;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    /// app + 一个拥有 `API_DEFINITION:READ+ADD` 的令牌。
    async fn app() -> (Router, String) {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms =
            PermissionSet::from_raw(["API_DEFINITION:READ+ADD".to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        let r = router(repo, sessions);
        (r, token)
    }

    fn post(uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    fn get(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).expect("req")
    }

    async fn json_body(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    async fn create_definition_returns_id(app: &Router, t: &str) -> String {
        let resp = app
            .clone()
            .oneshot(post(
                "/api/definition",
                r#"{"projectId":"p1","name":"登录","method":"POST","path":"/login"}"#,
                Some(t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = json_body(resp).await;
        v["id"].as_str().expect("id").to_string()
    }

    #[tokio::test]
    async fn create_definition_201() {
        let (app, t) = app().await;
        let resp = app
            .clone()
            .oneshot(post(
                "/api/definition",
                r#"{"projectId":"p1","name":"登录","method":"post","path":"/login"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = json_body(resp).await;
        assert_eq!(v["method"], "POST");
        assert_eq!(v["protocol"], "HTTP");
        assert_eq!(v["status"], "DRAFT");
    }

    #[tokio::test]
    async fn create_definition_without_token_401() {
        let (app, _t) = app().await;
        let resp = app
            .oneshot(post(
                "/api/definition",
                r#"{"projectId":"p1","name":"x"}"#,
                None,
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_definition_without_permission_403() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["API_DEFINITION:READ".to_string()]).expect("perms");
        let token = sessions.create("viewer", perms, 3600).await.expect("token");
        let app = router(repo, sessions);
        let resp = app
            .oneshot(post(
                "/api/definition",
                r#"{"projectId":"p1","name":"x","method":"GET"}"#,
                Some(&token),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn create_definition_invalid_method_400() {
        let (app, t) = app().await;
        let resp = app
            .oneshot(post(
                "/api/definition",
                r#"{"projectId":"p1","name":"x","method":"FETCH"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn create_definition_unknown_protocol_400() {
        let (app, t) = app().await;
        let resp = app
            .oneshot(post(
                "/api/definition",
                r#"{"projectId":"p1","name":"x","protocol":"grpc","method":"GET"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_and_get_definition() {
        let (app, t) = app().await;
        let id = create_definition_returns_id(&app, &t).await;

        // 列表为读端点,无需令牌
        let resp = app.clone().oneshot(get("/api/definition?projectId=p1")).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_body(resp).await;
        assert_eq!(v.as_array().expect("arr").len(), 1);

        // 按 id 取
        let resp = app
            .clone()
            .oneshot(get(&format!("/api/definition/{id}")))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);

        // 不存在 404
        let resp = app.oneshot(get("/api/definition/ghost")).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn add_case_201_and_list() {
        let (app, t) = app().await;
        let id = create_definition_returns_id(&app, &t).await;
        let resp = app
            .clone()
            .oneshot(post(
                &format!("/api/definition/{id}/case"),
                r#"{"name":"用例","method":"POST","url":"/login","assertions":[]}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = json_body(resp).await;
        assert_eq!(v["projectId"], "p1");

        let resp = app
            .oneshot(get(&format!("/api/definition/{id}/case")))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_body(resp).await;
        assert_eq!(v.as_array().expect("arr").len(), 1);
    }

    #[tokio::test]
    async fn add_case_to_missing_definition_404() {
        let (app, t) = app().await;
        let resp = app
            .oneshot(post(
                "/api/definition/ghost/case",
                r#"{"name":"用例","method":"GET","url":"/x"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn add_case_bad_payload_400() {
        let (app, t) = app().await;
        let id = create_definition_returns_id(&app, &t).await;
        // assertions 不是数组
        let resp = app
            .oneshot(post(
                &format!("/api/definition/{id}/case"),
                r#"{"name":"用例","method":"GET","url":"/x","assertions":{}}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn standalone_case_201_without_definition() {
        let (app, t) = app().await;
        let resp = app
            .clone()
            .oneshot(post(
                "/api/case",
                r#"{"projectId":"p1","name":"用例","method":"post","url":"/login"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = json_body(resp).await;
        assert_eq!(v["projectId"], "p1");
        assert_eq!(v["apiDefinitionId"], "");
        assert_eq!(v["method"], "POST");
    }

    #[tokio::test]
    async fn standalone_case_201_with_definition() {
        let (app, t) = app().await;
        let resp = app
            .clone()
            .oneshot(post(
                "/api/case",
                r#"{"projectId":"p1","apiDefinitionId":"def-9","name":"用例","method":"GET","url":"/x","assertions":[]}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = json_body(resp).await;
        assert_eq!(v["apiDefinitionId"], "def-9");
    }

    #[tokio::test]
    async fn standalone_case_without_token_401() {
        let (app, _t) = app().await;
        let resp = app
            .oneshot(post(
                "/api/case",
                r#"{"projectId":"p1","name":"用例","method":"GET","url":"/x"}"#,
                None,
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn standalone_case_without_permission_403() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["API_DEFINITION:READ".to_string()]).expect("perms");
        let token = sessions.create("viewer", perms, 3600).await.expect("token");
        let app = router(repo, sessions);
        let resp = app
            .oneshot(post(
                "/api/case",
                r#"{"projectId":"p1","name":"用例","method":"GET","url":"/x"}"#,
                Some(&token),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn standalone_case_bad_payload_400() {
        let (app, t) = app().await;
        let resp = app
            .oneshot(post(
                "/api/case",
                r#"{"projectId":"p1","name":"用例","method":"GET","url":"/x","assertions":{}}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_project_cases_paginated() {
        let (app, t) = app().await;
        for i in 1..=3 {
            let _ = app
                .clone()
                .oneshot(post(
                    "/api/case",
                    &format!(
                        r#"{{"projectId":"p1","name":"c{i}","method":"GET","url":"/x"}}"#
                    ),
                    Some(&t),
                ))
                .await
                .expect("seed");
        }

        // 读端点,无需令牌
        let resp = app
            .oneshot(get("/api/case?projectId=p1&current=1&pageSize=2"))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_body(resp).await;
        assert_eq!(v["total"], 3);
        assert_eq!(v["totalPages"], 2); // ceil(3/2)
        assert_eq!(v["items"].as_array().expect("arr").len(), 2);
    }

    #[tokio::test]
    async fn list_project_cases_bad_page_params_400() {
        let (app, _t) = app().await;
        let resp = app
            .oneshot(get("/api/case?projectId=p1&current=0&pageSize=10"))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn add_mock_201_and_list() {
        let (app, t) = app().await;
        let id = create_definition_returns_id(&app, &t).await;
        let resp = app
            .clone()
            .oneshot(post(
                &format!("/api/definition/{id}/mock"),
                r#"{"name":"挡板","responseStatus":404,"enabled":false}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = json_body(resp).await;
        assert_eq!(v["responseStatus"], 404);
        assert_eq!(v["enabled"], false);
        assert_eq!(v["matchRule"], serde_json::json!({}));

        let resp = app
            .oneshot(get(&format!("/api/definition/{id}/mock")))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_body(resp).await;
        assert_eq!(v.as_array().expect("arr").len(), 1);
    }

    #[tokio::test]
    async fn add_mock_bad_status_400() {
        let (app, t) = app().await;
        let id = create_definition_returns_id(&app, &t).await;
        let resp = app
            .oneshot(post(
                &format!("/api/definition/{id}/mock"),
                r#"{"name":"挡板","responseStatus":700}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn add_mock_to_missing_definition_404() {
        let (app, t) = app().await;
        let resp = app
            .oneshot(post("/api/definition/ghost/mock", r#"{"name":"挡板"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
