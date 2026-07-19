use std::sync::Arc;

use crate::application::{
    report_html, report_markdown, CreatePlanError, CreatePlanUseCase, PlanAdminError,
    PlanAdminUseCase, PlanCaseUseCase, PlanPatch, PlanStatisticsError, PlanStatisticsUseCase,
};
use crate::domain::{
    AssertionResult, CaseResult, CaseStatus, Plan, PlanMeta, PlanType, ROOT_GROUP,
};
use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct PlanState {
    create: CreatePlanUseCase,
    stats: PlanStatisticsUseCase,
    cases: PlanCaseUseCase,
    admin: PlanAdminUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<PlanState> for Arc<dyn SessionStore> {
    fn from_ref(s: &PlanState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    create: CreatePlanUseCase,
    stats: PlanStatisticsUseCase,
    cases: PlanCaseUseCase,
    admin: PlanAdminUseCase,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/test-plan", post(create_plan).get(list_plans))
        .route("/test-plan/{id}", get(plan_detail).put(update_plan))
        .route("/test-plan/{id}/planning", put(save_planning))
        .route("/test-plan/{id}/statistics", get(statistics))
        .route("/test-plan/{id}/report", get(report))
        .route("/test-plan/{id}/report.md", get(report_md))
        .route("/test-plan/{id}/cases", post(link_case).get(list_cases))
        .route("/test-plan/{id}/cases/{caseId}", axum::routing::delete(unlink_case))
        .route("/test-plan/{id}/cases/{caseId}/result", post(record_result))
        .with_state(PlanState { create, stats, cases, admin, sessions })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CreatePlanRequest {
    project_id: String,
    name: String,
    #[serde(rename = "type")]
    plan_type: String,
    #[serde(default = "default_group")]
    group_id: String,
}

fn default_group() -> String {
    ROOT_GROUP.to_string()
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PlanResponse {
    id: String,
    project_id: String,
    name: String,
    #[serde(rename = "type")]
    plan_type: String,
    group_id: String,
}

impl From<Plan> for PlanResponse {
    fn from(p: Plan) -> Self {
        Self {
            id: p.id,
            project_id: p.project_id,
            name: p.name,
            plan_type: p.plan_type.as_str().to_string(),
            group_id: p.group_id,
        }
    }
}

#[utoipa::path(post, path = "/test-plan", tag = "test-plan", request_body = CreatePlanRequest, responses((status = 201, body = PlanResponse), (status = 400)), security(("bearer" = [])))]
async fn create_plan(
    user: AuthUser,
    State(st): State<PlanState>,
    Json(req): Json<CreatePlanRequest>,
) -> Response {
    if !user.can("TEST_PLAN", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let Some(plan_type) = PlanType::parse(&req.plan_type) else {
        return (StatusCode::BAD_REQUEST, "unknown plan type").into_response();
    };
    match st
        .create
        .execute_as(&req.project_id, &req.name, plan_type, &req.group_id, Some(&user.user_id))
        .await
    {
        Ok(p) => (StatusCode::CREATED, Json(PlanResponse::from(p))).into_response(),
        Err(CreatePlanError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid plan payload").into_response()
        }
        Err(CreatePlanError::InvalidGroup) => {
            (StatusCode::BAD_REQUEST, "group not found or not a group").into_response()
        }
        Err(CreatePlanError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct ListPlansQuery {
    project_id: String,
}

/// Plan list row: core plan + the meta the list page renders, so the frontend
/// needs no per-plan detail calls.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PlanListItemResponse {
    id: String,
    project_id: String,
    name: String,
    #[serde(rename = "type")]
    plan_type: String,
    group_id: String,
    created_by: Option<String>,
    /// Unix milliseconds.
    created_at: i64,
    description: String,
    tags: Vec<String>,
    module_id: Option<String>,
    /// Unix milliseconds; null = not set.
    start_at: Option<i64>,
    end_at: Option<i64>,
    /// Percent 0..=100.
    pass_threshold: f64,
}

impl From<(Plan, PlanMeta)> for PlanListItemResponse {
    fn from((plan, meta): (Plan, PlanMeta)) -> Self {
        Self {
            id: plan.id,
            project_id: plan.project_id,
            name: plan.name,
            plan_type: plan.plan_type.as_str().to_string(),
            group_id: meta.group_id,
            created_by: plan.created_by,
            created_at: plan.created_at_ms,
            description: meta.description,
            tags: meta.tags,
            module_id: meta.module_id,
            start_at: meta.start_at_ms,
            end_at: meta.end_at_ms,
            pass_threshold: meta.pass_threshold * 100.0,
        }
    }
}

#[utoipa::path(get, path = "/test-plan", tag = "test-plan", params(ListPlansQuery), responses((status = 200, body = [PlanListItemResponse]), (status = 403)), security(("bearer" = [])))]
async fn list_plans(
    user: AuthUser,
    State(st): State<PlanState>,
    Query(q): Query<ListPlansQuery>,
) -> Response {
    if !user.can("TEST_PLAN", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.list(&q.project_id).await {
        Ok(items) => {
            let items: Vec<PlanListItemResponse> =
                items.into_iter().map(PlanListItemResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(e) => admin_error(e),
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PlanDetailResponse {
    id: String,
    project_id: String,
    name: String,
    #[serde(rename = "type")]
    plan_type: String,
    group_id: String,
    archived: bool,
    /// Unix milliseconds.
    created_at: i64,
    description: String,
    tags: Vec<String>,
    module_id: Option<String>,
    /// Unix milliseconds; null = not set.
    start_at: Option<i64>,
    end_at: Option<i64>,
    allow_duplicate_cases: bool,
    auto_update_status: bool,
    /// Percent 0..=100.
    pass_threshold: f64,
    /// Mind-map planning doc, stored verbatim; null until first saved.
    #[schema(value_type = Object, nullable)]
    planning: Option<serde_json::Value>,
}

fn detail_response(
    plan: Plan,
    meta: PlanMeta,
    planning: Option<serde_json::Value>,
) -> PlanDetailResponse {
    PlanDetailResponse {
        id: plan.id,
        project_id: plan.project_id,
        name: plan.name,
        plan_type: plan.plan_type.as_str().to_string(),
        group_id: meta.group_id.clone(),
        archived: plan.archived,
        created_at: plan.created_at_ms,
        description: meta.description,
        tags: meta.tags,
        module_id: meta.module_id,
        start_at: meta.start_at_ms,
        end_at: meta.end_at_ms,
        allow_duplicate_cases: meta.allow_duplicate_cases,
        auto_update_status: meta.auto_update_status,
        pass_threshold: meta.pass_threshold * 100.0,
        planning,
    }
}

#[utoipa::path(get, path = "/test-plan/{id}", tag = "test-plan", params(("id" = String, Path)), responses((status = 200, body = PlanDetailResponse), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn plan_detail(
    user: AuthUser,
    State(st): State<PlanState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("TEST_PLAN", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.detail(&id).await {
        Ok((plan, meta, planning)) => {
            (StatusCode::OK, Json(detail_response(plan, meta, planning))).into_response()
        }
        Err(e) => admin_error(e),
    }
}

/// Absent fields are unchanged. Clearing sentinels: moduleId "" clears the module,
/// groupId ""/"NONE" moves back to the root, startAt/endAt <= 0 clears the date.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct UpdatePlanRequest {
    name: Option<String>,
    description: Option<String>,
    tags: Option<Vec<String>>,
    module_id: Option<String>,
    group_id: Option<String>,
    /// Unix milliseconds.
    start_at: Option<i64>,
    end_at: Option<i64>,
    allow_duplicate_cases: Option<bool>,
    auto_update_status: Option<bool>,
    /// Percent 0..=100.
    pass_threshold: Option<f64>,
    /// true archives the plan (hidden from the list), false restores it.
    archived: Option<bool>,
}

#[utoipa::path(put, path = "/test-plan/{id}", tag = "test-plan", params(("id" = String, Path)), request_body = UpdatePlanRequest, responses((status = 200, body = PlanDetailResponse), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_plan(
    user: AuthUser,
    State(st): State<PlanState>,
    Path(id): Path<String>,
    Json(b): Json<UpdatePlanRequest>,
) -> Response {
    if !user.can("TEST_PLAN", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let patch = PlanPatch {
        name: b.name,
        description: b.description,
        tags: b.tags,
        module_id: b.module_id,
        group_id: b.group_id,
        start_at_ms: b.start_at,
        end_at_ms: b.end_at,
        allow_duplicate_cases: b.allow_duplicate_cases,
        auto_update_status: b.auto_update_status,
        pass_threshold: b.pass_threshold,
        archived: b.archived,
    };
    match st.admin.update(&id, patch).await {
        Ok((plan, meta, planning)) => {
            (StatusCode::OK, Json(detail_response(plan, meta, planning))).into_response()
        }
        Err(e) => admin_error(e),
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct SavePlanningResponse {
    /// Node-linked case/scenario ids synced into the flat plan-case links.
    linked_cases: usize,
}

#[utoipa::path(put, path = "/test-plan/{id}/planning", tag = "test-plan", params(("id" = String, Path)), request_body = Object, responses((status = 200, body = SavePlanningResponse), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn save_planning(
    user: AuthUser,
    State(st): State<PlanState>,
    Path(id): Path<String>,
    Json(doc): Json<serde_json::Value>,
) -> Response {
    if !user.can("TEST_PLAN", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.save_planning(&id, doc).await {
        Ok(linked) => {
            (StatusCode::OK, Json(SavePlanningResponse { linked_cases: linked })).into_response()
        }
        Err(e) => admin_error(e),
    }
}

fn admin_error(e: PlanAdminError) -> Response {
    match e {
        PlanAdminError::NotFound => (StatusCode::NOT_FOUND, "plan not found").into_response(),
        PlanAdminError::EmptyName => {
            (StatusCode::BAD_REQUEST, "plan name must not be empty").into_response()
        }
        PlanAdminError::Invalid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
        PlanAdminError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct StatisticsResponse {
    status: String,
    total: u64,
    pass_rate: f64,
    execute_rate: f64,
    is_pass: bool,
}

#[utoipa::path(get, path = "/test-plan/{id}/statistics", tag = "test-plan", params(("id" = String, Path)), responses((status = 200, body = StatisticsResponse), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn statistics(
    user: AuthUser,
    State(st): State<PlanState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("TEST_PLAN", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.stats.execute(&id).await {
        Ok(s) => (
            StatusCode::OK,
            Json(StatisticsResponse {
                status: s.status.as_str().to_string(),
                total: s.total,
                pass_rate: s.pass_rate,
                execute_rate: s.execute_rate,
                is_pass: s.is_pass,
            }),
        )
            .into_response(),
        Err(PlanStatisticsError::PlanNotFound) => {
            (StatusCode::NOT_FOUND, "plan not found").into_response()
        }
        Err(PlanStatisticsError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/test-plan/{id}/report", tag = "test-plan", params(("id" = String, Path)), responses((status = 200, description = "HTML 报告"), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn report(user: AuthUser, State(st): State<PlanState>, Path(id): Path<String>) -> Response {
    if !user.can("TEST_PLAN", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.stats.with_name(&id).await {
        Ok((name, s)) => {
            let cases = st.cases.list(&id).await.unwrap_or_default();
            (
                StatusCode::OK,
                [("content-type", "text/html; charset=utf-8")],
                report_html(&name, &s, &cases),
            )
                .into_response()
        }
        Err(PlanStatisticsError::PlanNotFound) => {
            (StatusCode::NOT_FOUND, "plan not found").into_response()
        }
        Err(PlanStatisticsError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/test-plan/{id}/report.md", tag = "test-plan", params(("id" = String, Path)), responses((status = 200, description = "Markdown 报告"), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn report_md(
    user: AuthUser,
    State(st): State<PlanState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("TEST_PLAN", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.stats.with_name(&id).await {
        Ok((name, s)) => {
            let cases = st.cases.list(&id).await.unwrap_or_default();
            (
                StatusCode::OK,
                [("content-type", "text/markdown; charset=utf-8")],
                report_markdown(&name, &s, &cases),
            )
                .into_response()
        }
        Err(PlanStatisticsError::PlanNotFound) => {
            (StatusCode::NOT_FOUND, "plan not found").into_response()
        }
        Err(PlanStatisticsError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct LinkCaseRequest {
    case_id: String,
    #[serde(default)]
    name: String,
}

#[utoipa::path(post, path = "/test-plan/{id}/cases", tag = "test-plan", params(("id" = String, Path)), request_body = LinkCaseRequest, responses((status = 201), (status = 403)), security(("bearer" = [])))]
async fn link_case(
    user: AuthUser,
    State(st): State<PlanState>,
    Path(id): Path<String>,
    Json(b): Json<LinkCaseRequest>,
) -> Response {
    if !user.can("TEST_PLAN", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.cases.link(&id, &b.case_id, &b.name).await {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(delete, path = "/test-plan/{id}/cases/{caseId}", tag = "test-plan", params(("id" = String, Path), ("caseId" = String, Path)), responses((status = 204), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn unlink_case(
    user: AuthUser,
    State(st): State<PlanState>,
    Path((id, case_id)): Path<(String, String)>,
) -> Response {
    if !user.can("TEST_PLAN", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.cases.unlink(&id, &case_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "case not linked to plan").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct AssertionResultDto {
    item: String,
    actual: String,
    condition: String,
    expected: String,
    passed: bool,
    #[serde(default)]
    reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RecordResultRequest {
    status: String,
    #[serde(default)]
    latency_ms: u64,
    #[serde(default)]
    response_size: u64,
    #[serde(default)]
    status_code: Option<i64>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assertions: Vec<AssertionResultDto>,
}

#[utoipa::path(post, path = "/test-plan/{id}/cases/{caseId}/result", tag = "test-plan", params(("id" = String, Path), ("caseId" = String, Path)), request_body = RecordResultRequest, responses((status = 200), (status = 404), (status = 403)), security(("bearer" = [])))]
async fn record_result(
    user: AuthUser,
    State(st): State<PlanState>,
    Path((id, case_id)): Path<(String, String)>,
    Json(b): Json<RecordResultRequest>,
) -> Response {
    if !user.can("TEST_PLAN", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let Some(status) = CaseStatus::parse(&b.status) else {
        return (StatusCode::BAD_REQUEST, "unknown status").into_response();
    };
    let result = CaseResult {
        latency_ms: b.latency_ms,
        response_size: b.response_size,
        status_code: b.status_code,
        body: b.body,
        assertions: b
            .assertions
            .into_iter()
            .map(|a| AssertionResult {
                item: a.item,
                actual: a.actual,
                condition: a.condition,
                expected: a.expected,
                passed: a.passed,
                reason: a.reason,
            })
            .collect(),
        ..Default::default()
    };
    match st.cases.record(&id, &case_id, status, Some(result)).await {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "case not linked to plan").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

/// Per-step result of an executed scenario mounted on the plan (recursive for controllers).
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct StepResultDto {
    name: String,
    kind: String,
    status: String,
    latency_ms: u64,
    status_code: Option<i64>,
    #[schema(no_recursion)]
    children: Vec<StepResultDto>,
}

impl From<&crate::domain::StepResult> for StepResultDto {
    fn from(s: &crate::domain::StepResult) -> Self {
        Self {
            name: s.name.clone(),
            kind: s.kind.clone(),
            status: s.status.as_str().to_string(),
            latency_ms: s.latency_ms,
            status_code: s.status_code,
            children: s.children.iter().map(Self::from).collect(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PlanCaseResponse {
    case_id: String,
    name: String,
    status: String,
    latency_ms: Option<u64>,
    status_code: Option<i64>,
    /// Non-empty for scenario-mounted entries executed by the plan runner.
    steps: Vec<StepResultDto>,
    /// Scenario report id for scenario-mounted entries; the full detail lives there.
    #[serde(skip_serializing_if = "Option::is_none")]
    report_id: Option<String>,
}

#[utoipa::path(get, path = "/test-plan/{id}/cases", tag = "test-plan", params(("id" = String, Path)), responses((status = 200, body = [PlanCaseResponse]), (status = 403)), security(("bearer" = [])))]
async fn list_cases(
    user: AuthUser,
    State(st): State<PlanState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("TEST_PLAN", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.cases.list(&id).await {
        Ok(cases) => {
            let items: Vec<PlanCaseResponse> = cases
                .into_iter()
                .map(|c| PlanCaseResponse {
                    case_id: c.case_id,
                    name: c.name,
                    status: c.status.as_str().to_string(),
                    latency_ms: c.result.as_ref().map(|r| r.latency_ms),
                    status_code: c.result.as_ref().and_then(|r| r.status_code),
                    steps: c
                        .result
                        .as_ref()
                        .map(|r| r.steps.iter().map(StepResultDto::from).collect())
                        .unwrap_or_default(),
                    report_id: c.result.as_ref().and_then(|r| r.report_id.clone()),
                })
                .collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(paths(create_plan, list_plans, plan_detail, update_plan, save_planning, statistics, report, report_md, link_case, unlink_case, record_result, list_cases), components(schemas(CreatePlanRequest, PlanResponse, PlanListItemResponse, PlanDetailResponse, UpdatePlanRequest, SavePlanningResponse, StatisticsResponse, LinkCaseRequest, RecordResultRequest, AssertionResultDto, PlanCaseResponse, StepResultDto)), tags((name = "test-plan", description = "测试计划")))]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryPlanRepository;
    use crate::domain::{CaseCounts, NewPlan};
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use std::sync::Arc;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app_with(repo: InMemoryPlanRepository) -> (Router, String) {
        let repo = Arc::new(repo);
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["TEST_PLAN:READ+ADD+UPDATE+EXECUTE".to_string()])
            .expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        let r = router(
            CreatePlanUseCase::new(repo.clone()),
            PlanStatisticsUseCase::new(repo.clone()),
            PlanCaseUseCase::new(repo.clone()),
            PlanAdminUseCase::new(repo),
            sessions,
        );
        (r, token)
    }

    fn post(uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b =
            Request::builder().method("POST").uri(uri).header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    #[tokio::test]
    async fn create_root_plan_201() {
        let (app, t) = app_with(InMemoryPlanRepository::new()).await;
        let resp = app
            .oneshot(post(
                "/test-plan",
                r#"{"projectId":"p1","name":"冒烟","type":"TEST_PLAN"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn create_without_token_401() {
        let (app, _t) = app_with(InMemoryPlanRepository::new()).await;
        let resp = app
            .oneshot(post(
                "/test-plan",
                r#"{"projectId":"p1","name":"x","type":"TEST_PLAN"}"#,
                None,
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_without_permission_403() {
        let repo = Arc::new(InMemoryPlanRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["TEST_PLAN:READ".to_string()]).expect("perms");
        let token = sessions.create("v", perms, 3600).await.expect("token");
        let app = router(
            CreatePlanUseCase::new(repo.clone()),
            PlanStatisticsUseCase::new(repo.clone()),
            PlanCaseUseCase::new(repo.clone()),
            PlanAdminUseCase::new(repo),
            sessions,
        );
        let resp = app
            .oneshot(post(
                "/test-plan",
                r#"{"projectId":"p1","name":"x","type":"TEST_PLAN"}"#,
                Some(&token),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn link_record_then_report_shows_case_detail() {
        let repo = InMemoryPlanRepository::new();
        let plan = repo
            .seed(NewPlan::new("p1", "冒烟", crate::domain::PlanType::Plan, ROOT_GROUP).expect("v"))
            .await;
        let (app, t) = app_with(repo).await;
        let pid = &plan.id;
        let r = app
            .clone()
            .oneshot(post(
                &format!("/test-plan/{pid}/cases"),
                r#"{"caseId":"c1","name":"健康检查"}"#,
                Some(&t),
            ))
            .await
            .expect("link");
        assert_eq!(r.status(), StatusCode::CREATED);
        let body = r#"{"status":"SUCCESS","latencyMs":12,"responseSize":3,"statusCode":200,"body":"ok","assertions":[{"item":"状态码","actual":"200","condition":"等于","expected":"200","passed":true}]}"#;
        let r = app
            .clone()
            .oneshot(post(&format!("/test-plan/{pid}/cases/c1/result"), body, Some(&t)))
            .await
            .expect("record");
        assert_eq!(r.status(), StatusCode::OK);
        let r = app
            .oneshot(
                Request::builder()
                    .uri(format!("/test-plan/{pid}/report"))
                    .header("authorization", format!("Bearer {t}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("report");
        assert_eq!(r.status(), StatusCode::OK);
        let html = String::from_utf8(
            axum::body::to_bytes(r.into_body(), usize::MAX).await.expect("b").to_vec(),
        )
        .expect("utf8");
        assert!(html.contains("健康检查"));
        assert!(html.contains("报告明细"));
        assert!(html.contains("断言项"));
        assert!(html.contains("通过率</span><b>100.0%"));
    }

    #[tokio::test]
    async fn unlink_case_204_then_404() {
        let repo = InMemoryPlanRepository::new();
        let plan = repo
            .seed(NewPlan::new("p1", "冒烟", crate::domain::PlanType::Plan, ROOT_GROUP).expect("v"))
            .await;
        let (app, t) = app_with(repo).await;
        let pid = &plan.id;
        let r = app
            .clone()
            .oneshot(post(
                &format!("/test-plan/{pid}/cases"),
                r#"{"caseId":"c1","name":"健康检查"}"#,
                Some(&t),
            ))
            .await
            .expect("link");
        assert_eq!(r.status(), StatusCode::CREATED);
        let del = |uri: String| {
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("authorization", format!("Bearer {t}"))
                .body(Body::empty())
                .expect("req")
        };
        let r = app.clone().oneshot(del(format!("/test-plan/{pid}/cases/c1"))).await.expect("del");
        assert_eq!(r.status(), StatusCode::NO_CONTENT);
        let r = app.oneshot(del(format!("/test-plan/{pid}/cases/c1"))).await.expect("del again");
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn record_on_unlinked_case_404() {
        let repo = InMemoryPlanRepository::new();
        let plan = repo
            .seed(NewPlan::new("p1", "x", crate::domain::PlanType::Plan, ROOT_GROUP).expect("v"))
            .await;
        let (app, t) = app_with(InMemoryPlanRepository::new()).await;
        let _ = plan;
        let r = app
            .oneshot(post("/test-plan/ghost/cases/c9/result", r#"{"status":"SUCCESS"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn create_nested_group_400() {
        let (app, t) = app_with(InMemoryPlanRepository::new()).await;
        let resp = app
            .oneshot(post(
                "/test-plan",
                r#"{"projectId":"p1","name":"组中组","type":"GROUP","groupId":"g1"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn create_unknown_type_400() {
        let (app, t) = app_with(InMemoryPlanRepository::new()).await;
        let resp = app
            .oneshot(post("/test-plan", r#"{"projectId":"p1","name":"x","type":"WAT"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn statistics_returns_rates() {
        let repo = InMemoryPlanRepository::new();
        let plan =
            repo.seed(NewPlan::new("p1", "冒烟", PlanType::Plan, ROOT_GROUP).expect("v")).await;
        repo.set_counts(
            &plan.id,
            CaseCounts { pending: 1, success: 2, error: 1, ..Default::default() },
        );
        repo.set_threshold(&plan.id, 0.5);

        let (app, t) = app_with(repo).await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/test-plan/{}/statistics", plan.id))
                    .header("authorization", format!("Bearer {t}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["status"], "UNDERWAY");
        assert_eq!(v["total"], 4);
        assert_eq!(v["isPass"], true);
    }

    fn put_req(uri: &str, body: &str, token: &str) -> Request<Body> {
        Request::builder()
            .method("PUT")
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body.to_string()))
            .expect("req")
    }

    async fn json_body(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    #[tokio::test]
    async fn detail_update_roundtrip() {
        let repo = InMemoryPlanRepository::new();
        let plan = repo
            .seed(NewPlan::new("p1", "冒烟", crate::domain::PlanType::Plan, ROOT_GROUP).expect("v"))
            .await;
        let (app, t) = app_with(repo).await;
        let body = r#"{"name":"回归","description":"说明","tags":["核心"],"moduleId":"m1","startAt":1000,"endAt":2000,"allowDuplicateCases":false,"autoUpdateStatus":false,"passThreshold":80}"#;
        let resp = app
            .clone()
            .oneshot(put_req(&format!("/test-plan/{}", plan.id), body, &t))
            .await
            .expect("put");
        assert_eq!(resp.status(), StatusCode::OK);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/test-plan/{}", plan.id))
                    .header("authorization", format!("Bearer {t}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("get");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_body(resp).await;
        assert_eq!(v["name"], "回归");
        assert_eq!(v["description"], "说明");
        assert_eq!(v["tags"], serde_json::json!(["核心"]));
        assert_eq!(v["moduleId"], "m1");
        assert_eq!(v["startAt"], 1000);
        assert_eq!(v["allowDuplicateCases"], false);
        assert_eq!(v["passThreshold"], 80.0);
        assert_eq!(v["planning"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn planning_roundtrip_and_link_sync() {
        let repo = InMemoryPlanRepository::new();
        let plan = repo
            .seed(NewPlan::new("p1", "冒烟", crate::domain::PlanType::Plan, ROOT_GROUP).expect("v"))
            .await;
        let (app, t) = app_with(repo).await;
        let doc = r#"{"nodes":[{"id":"n1","name":"接口用例","kind":"category","children":[{"id":"n2","name":"登录","kind":"point","caseIds":["c1"],"scenarioIds":["s1"],"config":{"mode":"serial"}}]}],"caseNames":{"c1":"健康检查"}}"#;
        let resp = app
            .clone()
            .oneshot(put_req(&format!("/test-plan/{}/planning", plan.id), doc, &t))
            .await
            .expect("put planning");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(json_body(resp).await["linkedCases"], 2);
        // GET detail returns the doc verbatim; the flat case list gained the links.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/test-plan/{}", plan.id))
                    .header("authorization", format!("Bearer {t}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("get");
        let v = json_body(resp).await;
        assert_eq!(v["planning"], serde_json::from_str::<serde_json::Value>(doc).expect("doc"));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/test-plan/{}/cases", plan.id))
                    .header("authorization", format!("Bearer {t}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("cases");
        let v = json_body(resp).await;
        assert_eq!(v.as_array().map(|a| a.len()), Some(2));
    }

    #[tokio::test]
    async fn planning_non_object_400_and_missing_plan_404() {
        let (app, t) = app_with(InMemoryPlanRepository::new()).await;
        let resp = app
            .clone()
            .oneshot(put_req("/test-plan/ghost/planning", "[1,2]", &t))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let resp = app
            .oneshot(put_req("/test-plan/ghost/planning", r#"{"nodes":[]}"#, &t))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn update_without_update_permission_403() {
        let repo = Arc::new(InMemoryPlanRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["TEST_PLAN:READ".to_string()]).expect("perms");
        let token = sessions.create("viewer", perms, 3600).await.expect("token");
        let app = router(
            CreatePlanUseCase::new(repo.clone()),
            PlanStatisticsUseCase::new(repo.clone()),
            PlanCaseUseCase::new(repo.clone()),
            PlanAdminUseCase::new(repo),
            sessions,
        );
        let resp =
            app.oneshot(put_req("/test-plan/x", r#"{"name":"y"}"#, &token)).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    fn get_req(uri: &str, token: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .expect("req")
    }

    #[tokio::test]
    async fn list_returns_created_plan_with_creator_and_meta() {
        let (app, t) = app_with(InMemoryPlanRepository::new()).await;
        let r = app
            .clone()
            .oneshot(post(
                "/test-plan",
                r#"{"projectId":"p1","name":"冒烟","type":"TEST_PLAN"}"#,
                Some(&t),
            ))
            .await
            .expect("create");
        assert_eq!(r.status(), StatusCode::CREATED);
        let id = json_body(r).await["id"].as_str().expect("id").to_string();
        // Group rows come from the same table and show up in the same list.
        let r = app
            .clone()
            .oneshot(post(
                "/test-plan",
                r#"{"projectId":"p1","name":"回归组","type":"GROUP"}"#,
                Some(&t),
            ))
            .await
            .expect("create group");
        assert_eq!(r.status(), StatusCode::CREATED);
        let body = r#"{"tags":["核心"],"moduleId":"m1","startAt":1000}"#;
        let r =
            app.clone().oneshot(put_req(&format!("/test-plan/{id}"), body, &t)).await.expect("put");
        assert_eq!(r.status(), StatusCode::OK);

        let r = app.oneshot(get_req("/test-plan?projectId=p1", &t)).await.expect("list");
        assert_eq!(r.status(), StatusCode::OK);
        let v = json_body(r).await;
        let items = v.as_array().expect("array");
        assert_eq!(items.len(), 2);
        // Newest first: the group was created after the plan.
        assert_eq!(items[0]["name"], "回归组");
        assert_eq!(items[0]["type"], "GROUP");
        let p = &items[1];
        assert_eq!(p["id"].as_str(), Some(id.as_str()));
        assert_eq!(p["createdBy"], "admin");
        assert_eq!(p["tags"], serde_json::json!(["核心"]));
        assert_eq!(p["moduleId"], "m1");
        assert_eq!(p["startAt"], 1000);
        assert_eq!(p["groupId"], "NONE");
        assert!(p["createdAt"].as_i64().is_some());
    }

    #[tokio::test]
    async fn list_scoped_to_project_and_hides_archived() {
        let (app, t) = app_with(InMemoryPlanRepository::new()).await;
        for body in [
            r#"{"projectId":"p1","name":"甲","type":"TEST_PLAN"}"#,
            r#"{"projectId":"p2","name":"乙","type":"TEST_PLAN"}"#,
        ] {
            let r = app.clone().oneshot(post("/test-plan", body, Some(&t))).await.expect("create");
            assert_eq!(r.status(), StatusCode::CREATED);
        }
        let r = app.clone().oneshot(get_req("/test-plan?projectId=p1", &t)).await.expect("list");
        let v = json_body(r).await;
        assert_eq!(v.as_array().map(|a| a.len()), Some(1));
        let id = v[0]["id"].as_str().expect("id").to_string();
        // Archive via PUT: the plan disappears from the list but the detail survives.
        let r = app
            .clone()
            .oneshot(put_req(&format!("/test-plan/{id}"), r#"{"archived":true}"#, &t))
            .await
            .expect("archive");
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(json_body(r).await["archived"], true);
        let r = app.clone().oneshot(get_req("/test-plan?projectId=p1", &t)).await.expect("list");
        assert_eq!(json_body(r).await.as_array().map(|a| a.len()), Some(0));
        let r = app.oneshot(get_req(&format!("/test-plan/{id}"), &t)).await.expect("detail");
        assert_eq!(r.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn list_without_token_401() {
        let (app, _t) = app_with(InMemoryPlanRepository::new()).await;
        let r = app
            .oneshot(
                Request::builder().uri("/test-plan?projectId=p1").body(Body::empty()).expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn statistics_missing_plan_404() {
        let (app, t) = app_with(InMemoryPlanRepository::new()).await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/test-plan/ghost/statistics")
                    .header("authorization", format!("Bearer {t}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
