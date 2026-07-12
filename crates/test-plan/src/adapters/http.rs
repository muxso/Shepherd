use std::sync::Arc;

use crate::application::{
    report_html, report_markdown, CreatePlanError, CreatePlanUseCase, PlanCaseUseCase,
    PlanStatisticsError, PlanStatisticsUseCase,
};
use crate::domain::{AssertionResult, CaseResult, CaseStatus, Plan, PlanType, ROOT_GROUP};
use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct PlanState {
    create: CreatePlanUseCase,
    stats: PlanStatisticsUseCase,
    cases: PlanCaseUseCase,
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
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/test-plan", post(create_plan))
        .route("/test-plan/{id}/statistics", get(statistics))
        .route("/test-plan/{id}/report", get(report))
        .route("/test-plan/{id}/report.md", get(report_md))
        .route("/test-plan/{id}/cases", post(link_case).get(list_cases))
        .route("/test-plan/{id}/cases/{caseId}/result", post(record_result))
        .with_state(PlanState { create, stats, cases, sessions })
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
    match st.create.execute(&req.project_id, &req.name, plan_type, &req.group_id).await {
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

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PlanCaseResponse {
    case_id: String,
    name: String,
    status: String,
    latency_ms: Option<u64>,
    status_code: Option<i64>,
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
                })
                .collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(paths(create_plan, statistics, report, report_md, link_case, record_result, list_cases), components(schemas(CreatePlanRequest, PlanResponse, StatisticsResponse, LinkCaseRequest, RecordResultRequest, AssertionResultDto, PlanCaseResponse)), tags((name = "test-plan", description = "测试计划")))]
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
        let perms =
            PermissionSet::from_raw(["TEST_PLAN:READ+ADD+EXECUTE".to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        let r = router(
            CreatePlanUseCase::new(repo.clone()),
            PlanStatisticsUseCase::new(repo.clone()),
            PlanCaseUseCase::new(repo),
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
            PlanCaseUseCase::new(repo),
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
        let (app, t) = {
            let repo = Arc::new(repo);
            let sessions = Arc::new(InMemorySessionStore::new());
            let perms =
                PermissionSet::from_raw(["TEST_PLAN:READ+ADD+EXECUTE".to_string()]).expect("p");
            let token = sessions.create("admin", perms, 3600).await.expect("tok");
            let r = router(
                CreatePlanUseCase::new(repo.clone()),
                PlanStatisticsUseCase::new(repo.clone()),
                PlanCaseUseCase::new(repo),
                sessions,
            );
            (r, token)
        };
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
