//! 场景管理的 HTTP 适配器。
//!
//! 编排/编译规则全在 domain/application;本层只做 DTO 翻译 + 错误码映射。
//! RBAC 资源 `API_SCENARIO`:写操作需 `API_SCENARIO:ADD`,读操作开放。
//! 错误映射:场景/子场景不存在 → 404,校验失败 → 400,编译成环 → 409。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use kernel::page::PageRequest;
use crate::application::{
    AddStepError, AddStepUseCase, CompileError, CompileScenarioUseCase, CreateScenarioError,
    CreateScenarioUseCase, GetScenarioUseCase, ListScenarioExecutionsUseCase, ListScenariosUseCase,
};
use crate::domain::{
    ApiScenario, ControlKind, InlineRequest, NewScenarioStep, RefMode, RunnableStep,
    ScenarioExecution, ScenarioStep, StepKind,
};
use crate::ports::ApiScenarioRepository;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct ScenarioAppState {
    create: CreateScenarioUseCase,
    list: ListScenariosUseCase,
    get: GetScenarioUseCase,
    add_step: AddStepUseCase,
    compile: CompileScenarioUseCase,
    list_executions: ListScenarioExecutionsUseCase,
    /// 直接用于 PATCH 更新场景基本信息(名称/状态/元信息);其余走用例。
    repo: Arc<dyn ApiScenarioRepository>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<ScenarioAppState> for Arc<dyn SessionStore> {
    fn from_ref(s: &ScenarioAppState) -> Self {
        s.sessions.clone()
    }
}

/// 由仓储 + 会话存储装配路由,内部构建各用例。
pub fn router(
    repo: Arc<dyn ApiScenarioRepository>,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    let state = ScenarioAppState {
        create: CreateScenarioUseCase::new(repo.clone()),
        list: ListScenariosUseCase::new(repo.clone()),
        get: GetScenarioUseCase::new(repo.clone()),
        add_step: AddStepUseCase::new(repo.clone()),
        compile: CompileScenarioUseCase::new(repo.clone()),
        list_executions: ListScenarioExecutionsUseCase::new(repo.clone()),
        repo,
        sessions,
    };
    Router::new()
        .route("/api/scenario", post(create_scenario).get(list_scenarios))
        .route("/api/scenario/{id}", get(get_scenario).patch(update_scenario).delete(delete_scenario))
        .route("/api/scenario/{id}/step", post(add_step))
        .route("/api/scenario/{id}/step/{step_id}", axum::routing::delete(delete_step))
        .route("/api/scenario/{id}/steps/order", axum::routing::patch(reorder_steps))
        .route("/api/scenario/{id}/compile", get(compile_scenario))
        .route("/api/scenario/{id}/executions", get(list_executions))
        .route("/api/scenario/{id}/changes", get(list_changes))
        .with_state(state)
}

// ---------------- DTO ----------------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ScenarioCreateBody {
    project_id: String,
    name: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct InlineRequestDto {
    method: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    /// 断言数组(api-runner Assertion 的序列化形式);空则省略。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schema(value_type = Vec<Object>)]
    assertions: Vec<serde_json::Value>,
    /// 请求头 / Query / REST 参数([{key,value}]);空则省略。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schema(value_type = Vec<Object>)]
    headers: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schema(value_type = Vec<Object>)]
    query_params: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schema(value_type = Vec<Object>)]
    rest_params: Vec<serde_json::Value>,
    /// 认证 {type,token};空对象则省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Object)]
    auth: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schema(value_type = Vec<Object>)]
    processors: Vec<serde_json::Value>,
}

impl From<&InlineRequest> for InlineRequestDto {
    fn from(r: &InlineRequest) -> Self {
        let arr = |v: &serde_json::Value| v.as_array().cloned().unwrap_or_default();
        Self {
            method: r.method.clone(),
            url: r.url.clone(),
            body: r.body.clone(),
            assertions: arr(&r.assertions),
            headers: arr(&r.headers),
            query_params: arr(&r.query_params),
            rest_params: arr(&r.rest_params),
            auth: r.auth.as_object().filter(|o| !o.is_empty()).map(|_| r.auth.clone()),
            processors: arr(&r.processors),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ScenarioStepResponse {
    id: String,
    order: i32,
    kind: String,
    ref_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    case_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scenario_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request: Option<InlineRequestDto>,
    /// 控制器载荷(LOOP/IF/ONCE/TIMER 步骤)。
    #[serde(skip_serializing_if = "Option::is_none")]
    control: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<serde_json::Value>,
}

impl From<&ScenarioStep> for ScenarioStepResponse {
    fn from(s: &ScenarioStep) -> Self {
        let (case_id, scenario_id, request, control) = match &s.kind {
            StepKind::Request(r) => (None, None, Some(InlineRequestDto::from(r)), None),
            StepKind::Case { case_id } => (Some(case_id.clone()), None, None, None),
            StepKind::Scenario { scenario_id } => (None, Some(scenario_id.clone()), None, None),
            StepKind::Control { payload, .. } => (None, None, None, Some(payload.clone())),
        };
        Self {
            id: s.id.clone(),
            order: s.order,
            kind: s.kind.kind_str().to_string(),
            ref_mode: s.ref_mode.as_str().to_string(),
            case_id,
            scenario_id,
            request,
            control,
            snapshot: s.snapshot.clone(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ScenarioResponse {
    id: String,
    project_id: String,
    name: String,
    status: String,
    /// 元信息(描述/标签/等级/模块/参数);不透明 JSON。
    meta: serde_json::Value,
    /// 审计:创建人/创建时间/更新时间(只读;见 0046 迁移)。
    created_by: Option<String>,
    created_at: String,
    updated_at: String,
    steps: Vec<ScenarioStepResponse>,
    /// 最近一次执行结果状态(SUCCESS/ERROR;列表回填,单查为 null)。
    #[serde(skip_serializing_if = "Option::is_none")]
    last_result: Option<String>,
}

impl From<ApiScenario> for ScenarioResponse {
    fn from(s: ApiScenario) -> Self {
        Self {
            id: s.id,
            project_id: s.project_id,
            name: s.name,
            status: s.status.as_str().to_string(),
            meta: s.meta,
            created_by: s.created_by,
            created_at: s.created_at,
            updated_at: s.updated_at,
            steps: s.steps.iter().map(ScenarioStepResponse::from).collect(),
            last_result: s.last_result,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ScenarioUpdateBody {
    name: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    meta: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct AddStepBody {
    /// "REQUEST" | "CASE" | "SCENARIO" | "LOOP" | "IF" | "ONCE" | "TIMER"
    kind: String,
    /// "REFERENCE" | "COPY",缺省 REFERENCE。
    #[serde(default)]
    ref_mode: Option<String>,
    order: i32,
    #[serde(default)]
    request: Option<InlineRequestBody>,
    #[serde(default)]
    ref_id: Option<String>,
    /// 控制器载荷(kind 为 LOOP/IF/ONCE/TIMER 时必填),如
    /// `{"times":3,"children":[{"kind":"CASE","refId":"c1"}]}`。
    #[serde(default)]
    control: Option<serde_json::Value>,
    #[serde(default)]
    snapshot: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct InlineRequestBody {
    method: String,
    url: String,
    #[serde(default)]
    body: Option<String>,
    /// 断言数组(api-runner Assertion 形式,如 `[{"type":"StatusIs","args":200}]`);默认空。
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    assertions: Option<serde_json::Value>,
    /// 请求头 / Query / REST 参数([{key,value}])/ 认证({type,token})/ 处理器(EXTRACT/WAIT);均可选。
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    headers: Option<serde_json::Value>,
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    query_params: Option<serde_json::Value>,
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    rest_params: Option<serde_json::Value>,
    #[serde(default)]
    #[schema(value_type = Object)]
    auth: Option<serde_json::Value>,
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    processors: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CompiledStepDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    case_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request: Option<InlineRequestDto>,
}

impl From<&RunnableStep> for CompiledStepDto {
    fn from(r: &RunnableStep) -> Self {
        Self {
            case_id: r.case_id.clone(),
            request: r.request.as_ref().map(InlineRequestDto::from),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CompileResultDto {
    steps: Vec<CompiledStepDto>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct ScenarioListQuery {
    project_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ScenarioExecutionDto {
    id: String,
    scenario_id: String,
    project_id: String,
    status: String,
    case_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    report_id: Option<String>,
    created_at: String,
}

impl From<ScenarioExecution> for ScenarioExecutionDto {
    fn from(e: ScenarioExecution) -> Self {
        Self {
            id: e.id,
            scenario_id: e.scenario_id,
            project_id: e.project_id,
            status: e.status.as_str().to_string(),
            case_count: e.case_count,
            report_id: e.report_id,
            created_at: e.created_at,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct ExecutionListQuery {
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
struct ScenarioExecutionPageResponse {
    total: u64,
    current: u32,
    page_size: u32,
    total_pages: u64,
    items: Vec<ScenarioExecutionDto>,
}

// ---------------- handlers ----------------

#[utoipa::path(post, path = "/api/scenario", tag = "api-scenario", request_body = ScenarioCreateBody, responses((status = 201, body = ScenarioResponse), (status = 400)), security(("bearer" = [])))]
async fn create_scenario(
    user: AuthUser,
    State(st): State<ScenarioAppState>,
    Json(req): Json<ScenarioCreateBody>,
) -> Response {
    if !user.can("API_SCENARIO", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.create.execute(&req.project_id, &req.name, Some(&user.user_id)).await {
        Ok(s) => {
            let _ = st.repo.record_change(&s.id, "CREATE", Some(&s.name), Some(&user.user_id)).await;
            (StatusCode::CREATED, Json(ScenarioResponse::from(s))).into_response()
        }
        Err(CreateScenarioError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid scenario payload").into_response()
        }
        Err(CreateScenarioError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/api/scenario", tag = "api-scenario", params(ScenarioListQuery), responses((status = 200, body = [ScenarioResponse])))]
async fn list_scenarios(
    State(st): State<ScenarioAppState>,
    Query(q): Query<ScenarioListQuery>,
) -> Response {
    match st.list.execute(&q.project_id).await {
        Ok(list) => {
            let body: Vec<ScenarioResponse> =
                list.into_iter().map(ScenarioResponse::from).collect();
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(get, path = "/api/scenario/{id}", tag = "api-scenario", params(("id" = String, Path)), responses((status = 200, body = ScenarioResponse), (status = 404)))]
async fn get_scenario(State(st): State<ScenarioAppState>, Path(id): Path<String>) -> Response {
    match st.get.execute(&id).await {
        Ok(Some(s)) => (StatusCode::OK, Json(ScenarioResponse::from(s))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "scenario not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(patch, path = "/api/scenario/{id}", tag = "api-scenario", params(("id" = String, Path)), request_body = ScenarioUpdateBody, responses((status = 200, body = ScenarioResponse), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn update_scenario(
    user: AuthUser,
    State(st): State<ScenarioAppState>,
    Path(id): Path<String>,
    Json(req): Json<ScenarioUpdateBody>,
) -> Response {
    if !user.can("API_SCENARIO", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let name = req.name.trim();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, "name required").into_response();
    }
    // 名称为一等列;状态缺省沿用已有(需先取);meta 整体替换(缺省空对象)。
    let status = match &req.status {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => match st.get.execute(&id).await {
            Ok(Some(s)) => s.status.as_str().to_string(),
            Ok(None) => return (StatusCode::NOT_FOUND, "scenario not found").into_response(),
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
        },
    };
    let meta = req.meta.clone().unwrap_or_else(|| serde_json::json!({}));
    match st.repo.update_scenario(&id, name, &status, &meta).await {
        Ok(Some(s)) => {
            let _ = st.repo.record_change(&id, "UPDATE", Some("更新基本信息/参数/设置"), Some(&user.user_id)).await;
            (StatusCode::OK, Json(ScenarioResponse::from(s))).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "scenario not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReorderStepsBody {
    /// 期望顺序的 step_id 列表(从第 1 位起写 step_order)。
    order: Vec<String>,
}

#[utoipa::path(patch, path = "/api/scenario/{id}/steps/order", tag = "api-scenario", params(("id" = String, Path)), request_body = ReorderStepsBody, responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn reorder_steps(
    user: AuthUser,
    State(st): State<ScenarioAppState>,
    Path(id): Path<String>,
    Json(req): Json<ReorderStepsBody>,
) -> Response {
    if !user.can("API_SCENARIO", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.repo.reorder_steps(&id, &req.order).await {
        Ok(()) => {
            let _ = st.repo.record_change(&id, "REORDER", Some("调整步骤顺序"), Some(&user.user_id)).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(delete, path = "/api/scenario/{id}/step/{step_id}", tag = "api-scenario", params(("id" = String, Path), ("step_id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn delete_step(
    user: AuthUser,
    State(st): State<ScenarioAppState>,
    Path((id, step_id)): Path<(String, String)>,
) -> Response {
    if !user.can("API_SCENARIO", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.repo.delete_step(&id, &step_id).await {
        Ok(true) => {
            let _ = st.repo.record_change(&id, "DELETE_STEP", Some("删除步骤"), Some(&user.user_id)).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => (StatusCode::NOT_FOUND, "step not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(delete, path = "/api/scenario/{id}", tag = "api-scenario", params(("id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn delete_scenario(
    user: AuthUser,
    State(st): State<ScenarioAppState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("API_SCENARIO", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.repo.delete_scenario(&id).await {
        Ok(true) => {
            let _ = st.repo.record_change(&id, "DELETE", Some("删除场景"), Some(&user.user_id)).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => (StatusCode::NOT_FOUND, "scenario not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(post, path = "/api/scenario/{id}/step", tag = "api-scenario", params(("id" = String, Path)), request_body = AddStepBody, responses((status = 201, body = ScenarioStepResponse), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn add_step(
    user: AuthUser,
    State(st): State<ScenarioAppState>,
    Path(id): Path<String>,
    Json(req): Json<AddStepBody>,
) -> Response {
    if !user.can("API_SCENARIO", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }

    // 据 kind 组装领域 StepKind(任何字段缺失/非法即 400)。
    let kind = match req.kind.as_str() {
        "REQUEST" => {
            let Some(r) = req.request else {
                return (StatusCode::BAD_REQUEST, "request payload required").into_response();
            };
            let empty = || serde_json::Value::Array(vec![]);
            match InlineRequest::new(&r.method, &r.url, r.body) {
                Ok(req) => StepKind::Request(
                    req.with_assertions(r.assertions.unwrap_or_else(empty)).with_spec(
                        r.headers.unwrap_or_else(empty),
                        r.query_params.unwrap_or_else(empty),
                        r.rest_params.unwrap_or_else(empty),
                        r.auth.unwrap_or_else(|| serde_json::json!({})),
                        r.processors.unwrap_or_else(empty),
                    ),
                ),
                Err(_) => {
                    return (StatusCode::BAD_REQUEST, "invalid inline request").into_response()
                }
            }
        }
        "CASE" => match req.ref_id {
            Some(case_id) => StepKind::Case { case_id },
            None => return (StatusCode::BAD_REQUEST, "refId required for CASE").into_response(),
        },
        "SCENARIO" => match req.ref_id {
            Some(scenario_id) => StepKind::Scenario { scenario_id },
            None => {
                return (StatusCode::BAD_REQUEST, "refId required for SCENARIO").into_response()
            }
        },
        "LOOP" | "IF" | "ONCE" | "TIMER" => {
            let Some(control) = ControlKind::parse(&req.kind) else {
                return (StatusCode::BAD_REQUEST, "unknown control kind").into_response();
            };
            let Some(payload) = req.control else {
                return (StatusCode::BAD_REQUEST, "control payload required").into_response();
            };
            StepKind::Control { control, payload }
        }
        _ => return (StatusCode::BAD_REQUEST, "unknown step kind").into_response(),
    };

    let ref_mode = req.ref_mode.as_deref().map(RefMode::parse).unwrap_or_default();
    let new_step = match NewScenarioStep::new(req.order, kind, ref_mode, req.snapshot) {
        Ok(s) => s,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid step payload").into_response(),
    };

    match st.add_step.execute(&id, &new_step).await {
        Ok(step) => {
            let _ = st.repo.record_change(&id, "ADD_STEP", Some(&format!("新增步骤 {}", req.kind)), Some(&user.user_id)).await;
            (StatusCode::CREATED, Json(ScenarioStepResponse::from(&step))).into_response()
        }
        Err(AddStepError::NotFound) => {
            (StatusCode::NOT_FOUND, "scenario not found").into_response()
        }
        Err(AddStepError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/api/scenario/{id}/compile", tag = "api-scenario", params(("id" = String, Path)), responses((status = 200, body = CompileResultDto), (status = 404), (status = 409)))]
async fn compile_scenario(
    State(st): State<ScenarioAppState>,
    Path(id): Path<String>,
) -> Response {
    match st.compile.execute(&id).await {
        Ok(steps) => {
            let body = CompileResultDto {
                steps: steps.iter().map(CompiledStepDto::from).collect(),
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(CompileError::NotFound(_)) => {
            (StatusCode::NOT_FOUND, "scenario not found").into_response()
        }
        Err(CompileError::Cycle(_)) => {
            (StatusCode::CONFLICT, "scenario cycle detected").into_response()
        }
        Err(CompileError::Depth(_)) => {
            (StatusCode::CONFLICT, "scenario max depth exceeded").into_response()
        }
        Err(CompileError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/api/scenario/{id}/executions", tag = "api-scenario", params(("id" = String, Path), ExecutionListQuery), responses((status = 200, body = ScenarioExecutionPageResponse), (status = 400)))]
async fn list_executions(
    State(st): State<ScenarioAppState>,
    Path(id): Path<String>,
    Query(q): Query<ExecutionListQuery>,
) -> Response {
    // 分页参数校验复用 kernel:非法参数 → 400(而不是打到存储才炸)。
    let page = match PageRequest::new(q.current, q.page_size) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid page params").into_response(),
    };
    match st.list_executions.execute(&id, page).await {
        Ok(page) => {
            let body = ScenarioExecutionPageResponse {
                total: page.total,
                current: page.current,
                page_size: page.page_size,
                total_pages: page.total_pages(),
                items: page.items.into_iter().map(ScenarioExecutionDto::from).collect(),
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ScenarioChangeDto {
    id: String,
    action: String,
    detail: Option<String>,
    user_id: Option<String>,
    created_at: String,
}

#[utoipa::path(get, path = "/api/scenario/{id}/changes", tag = "api-scenario", params(("id" = String, Path)), responses((status = 200, body = [ScenarioChangeDto])))]
async fn list_changes(State(st): State<ScenarioAppState>, Path(id): Path<String>) -> Response {
    match st.repo.list_changes(&id).await {
        Ok(list) => {
            let body: Vec<ScenarioChangeDto> = list
                .into_iter()
                .map(|c| ScenarioChangeDto {
                    id: c.id,
                    action: c.action,
                    detail: c.detail,
                    user_id: c.user_id,
                    created_at: c.created_at,
                })
                .collect();
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(create_scenario, list_scenarios, get_scenario, delete_scenario, add_step, compile_scenario, list_executions),
    components(schemas(
        ScenarioCreateBody,
        ScenarioResponse,
        ScenarioStepResponse,
        AddStepBody,
        InlineRequestBody,
        InlineRequestDto,
        CompiledStepDto,
        CompileResultDto,
        ScenarioExecutionDto,
        ScenarioExecutionPageResponse
    )),
    tags((name = "api-scenario", description = "接口场景编排"))
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
    use crate::adapters::InMemoryApiScenarioRepository;
    use kernel::permission::PermissionSet;
    use std::sync::Arc;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    /// app + 一个拥有 `API_SCENARIO:READ+ADD` 的令牌。
    async fn app() -> (Router, String) {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms =
            PermissionSet::from_raw(["API_SCENARIO:READ+ADD".to_string()]).expect("perms");
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

    fn get_req(uri: &str) -> Request<Body> {
        Request::builder().method("GET").uri(uri).body(Body::empty()).expect("req")
    }

    async fn json_body(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    async fn create_scenario_id(app: &Router, t: &str) -> String {
        let resp = app
            .clone()
            .oneshot(post("/api/scenario", r#"{"projectId":"p1","name":"下单"}"#, Some(t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = json_body(resp).await;
        v["id"].as_str().expect("id").to_string()
    }

    #[tokio::test]
    async fn create_scenario_201() {
        let (app, t) = app().await;
        let id = create_scenario_id(&app, &t).await;
        assert!(id.starts_with("scn-"));
    }

    #[tokio::test]
    async fn create_without_token_401() {
        let (app, _t) = app().await;
        let resp = app
            .oneshot(post("/api/scenario", r#"{"projectId":"p1","name":"x"}"#, None))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_without_permission_403() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["API_SCENARIO:READ".to_string()]).expect("perms");
        let token = sessions.create("u", perms, 3600).await.expect("token");
        let app = router(repo, sessions);
        let resp = app
            .oneshot(post("/api/scenario", r#"{"projectId":"p1","name":"x"}"#, Some(&token)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn create_blank_name_400() {
        let (app, t) = app().await;
        let resp = app
            .oneshot(post("/api/scenario", r#"{"projectId":"p1","name":"  "}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_scenarios_open_200() {
        let (app, t) = app().await;
        create_scenario_id(&app, &t).await;
        let resp =
            app.oneshot(get_req("/api/scenario?projectId=p1")).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_body(resp).await;
        assert_eq!(v.as_array().expect("array").len(), 1);
    }

    #[tokio::test]
    async fn get_scenario_with_steps_200() {
        let (app, t) = app().await;
        let id = create_scenario_id(&app, &t).await;
        app.clone()
            .oneshot(post(
                &format!("/api/scenario/{id}/step"),
                r#"{"kind":"CASE","order":0,"refId":"case-7"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        let resp =
            app.oneshot(get_req(&format!("/api/scenario/{id}"))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_body(resp).await;
        assert_eq!(v["steps"][0]["kind"], "CASE");
        assert_eq!(v["steps"][0]["caseId"], "case-7");
    }

    #[tokio::test]
    async fn get_missing_scenario_404() {
        let (app, _t) = app().await;
        let resp = app.oneshot(get_req("/api/scenario/ghost")).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn add_request_step_201() {
        let (app, t) = app().await;
        let id = create_scenario_id(&app, &t).await;
        let resp = app
            .oneshot(post(
                &format!("/api/scenario/{id}/step"),
                r#"{"kind":"REQUEST","order":0,"request":{"method":"GET","url":"http://x"}}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = json_body(resp).await;
        assert_eq!(v["kind"], "REQUEST");
        assert_eq!(v["request"]["method"], "GET");
    }

    #[tokio::test]
    async fn add_step_invalid_method_400() {
        let (app, t) = app().await;
        let id = create_scenario_id(&app, &t).await;
        let resp = app
            .oneshot(post(
                &format!("/api/scenario/{id}/step"),
                r#"{"kind":"REQUEST","order":0,"request":{"method":"FETCH","url":"http://x"}}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn add_copy_step_without_snapshot_400() {
        let (app, t) = app().await;
        let id = create_scenario_id(&app, &t).await;
        let resp = app
            .oneshot(post(
                &format!("/api/scenario/{id}/step"),
                r#"{"kind":"CASE","order":0,"refId":"c1","refMode":"COPY"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn add_step_to_missing_scenario_404() {
        let (app, t) = app().await;
        let resp = app
            .oneshot(post(
                "/api/scenario/ghost/step",
                r#"{"kind":"CASE","order":0,"refId":"c1"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn compile_flat_scenario_200() {
        let (app, t) = app().await;
        let id = create_scenario_id(&app, &t).await;
        app.clone()
            .oneshot(post(
                &format!("/api/scenario/{id}/step"),
                r#"{"kind":"CASE","order":0,"refId":"case-1"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        let resp = app
            .oneshot(get_req(&format!("/api/scenario/{id}/compile")))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_body(resp).await;
        assert_eq!(v["steps"][0]["caseId"], "case-1");
    }

    #[tokio::test]
    async fn compile_cycle_409() {
        let (app, t) = app().await;
        let id = create_scenario_id(&app, &t).await;
        // 自引用 → 成环
        app.clone()
            .oneshot(post(
                &format!("/api/scenario/{id}/step"),
                &format!(r#"{{"kind":"SCENARIO","order":0,"refId":"{id}"}}"#),
                Some(&t),
            ))
            .await
            .expect("resp");
        let resp = app
            .oneshot(get_req(&format!("/api/scenario/{id}/compile")))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn compile_missing_scenario_404() {
        let (app, _t) = app().await;
        let resp =
            app.oneshot(get_req("/api/scenario/ghost/compile")).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn openapi_exposes_unique_schemas() {
        let doc = openapi();
        let json = serde_json::to_value(&doc).expect("serialize");
        let schemas = json["components"]["schemas"].as_object().expect("schemas");
        for name in [
            "ScenarioCreateBody",
            "ScenarioResponse",
            "ScenarioStepResponse",
            "AddStepBody",
            "InlineRequestDto",
            "CompiledStepDto",
            "CompileResultDto",
            "ScenarioExecutionDto",
            "ScenarioExecutionPageResponse",
        ] {
            assert!(schemas.contains_key(name), "missing schema {name}");
        }
    }

    /// 构造 app 并交出底层仓储,便于直接记录执行(HTTP 无写执行端点)。
    async fn app_with_repo() -> (Router, Arc<InMemoryApiScenarioRepository>) {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let r = router(repo.clone(), sessions);
        (r, repo)
    }

    #[tokio::test]
    async fn list_executions_returns_paginated_body() {
        use crate::ports::ApiScenarioRepository;
        let (app, repo) = app_with_repo().await;
        // 记录 3 条执行(状态各异),另一场景 1 条不应混入。
        for i in 0..3 {
            let status = if i == 2 { "SUCCESS" } else { "PENDING" };
            repo.record_execution("scn-1", "p1", status, i, None).await.expect("rec");
        }
        repo.record_execution("scn-2", "p1", "ERROR", 0, None).await.expect("rec");

        // 读端点,无需令牌。
        let resp = app
            .oneshot(get_req("/api/scenario/scn-1/executions?current=1&pageSize=2"))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_body(resp).await;
        assert_eq!(v["total"], 3);
        assert_eq!(v["totalPages"], 2); // ceil(3/2)
        assert_eq!(v["items"].as_array().expect("arr").len(), 2);
        // 最新(SUCCESS)在前;status 字段存在并往返。
        assert_eq!(v["items"][0]["status"], "SUCCESS");
        assert_eq!(v["items"][0]["scenarioId"], "scn-1");
    }

    #[tokio::test]
    async fn list_executions_empty_ok() {
        let (app, _repo) = app_with_repo().await;
        let resp = app
            .oneshot(get_req("/api/scenario/scn-x/executions"))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_body(resp).await;
        assert_eq!(v["total"], 0);
        assert_eq!(v["totalPages"], 0);
        assert_eq!(v["items"].as_array().expect("arr").len(), 0);
    }

    #[tokio::test]
    async fn list_executions_bad_page_params_400() {
        let (app, _repo) = app_with_repo().await;
        let resp = app
            .oneshot(get_req("/api/scenario/scn-1/executions?current=0&pageSize=10"))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
