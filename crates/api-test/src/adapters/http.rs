use std::sync::Arc;

use crate::application::{
    CreateResourcePoolError, CreateResourcePoolUseCase, EditResourcePoolUseCase,
    ListCaseExecutionsUseCase, ListResourcePoolsUseCase, StartBatchRunUseCase,
};
use crate::domain::{BatchRunError, BatchRunMode, ResourcePool, ResourcePoolDraft, RunModeConfig};
use crate::ports::{CaseExecutionRecord, PortError};
use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use kernel::page::PageRequest;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct BatchRunState {
    uc: StartBatchRunUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<BatchRunState> for Arc<dyn SessionStore> {
    fn from_ref(s: &BatchRunState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(use_case: StartBatchRunUseCase, sessions: Arc<dyn SessionStore>) -> Router {
    let state = BatchRunState { uc: use_case, sessions };
    Router::new()
        .route("/api/batch-run", post(batch_run))
        .route("/api/case/{id}/run", post(run_case))
        .with_state(state)
}

#[derive(Clone)]
struct ExecutionsState {
    uc: ListCaseExecutionsUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<ExecutionsState> for Arc<dyn SessionStore> {
    fn from_ref(s: &ExecutionsState) -> Self {
        s.sessions.clone()
    }
}

pub fn executions_router(uc: ListCaseExecutionsUseCase, sessions: Arc<dyn SessionStore>) -> Router {
    let state = ExecutionsState { uc, sessions };
    Router::new().route("/api/case/{caseId}/executions", get(list_executions)).with_state(state)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct BatchRunRequest {
    project_id: String,
    case_ids: Vec<String>,
    run_mode: String,
    #[serde(default)]
    pool_id: Option<String>,
    #[serde(default)]
    environment_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct BatchRunResponse {
    report_id: String,
    status: String,
}

#[utoipa::path(post, path = "/api/batch-run", tag = "api-test", request_body = BatchRunRequest, responses((status = 200, body = BatchRunResponse), (status = 400), (status = 403), (status = 409)), security(("bearer" = [])))]
async fn batch_run(
    user: AuthUser,
    State(st): State<BatchRunState>,
    Json(req): Json<BatchRunRequest>,
) -> Response {
    if !user.can("API_DEFINITION", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let Some(mode) = BatchRunMode::parse(&req.run_mode) else {
        return (StatusCode::BAD_REQUEST, "unknown run mode").into_response();
    };
    let config = RunModeConfig {
        mode,
        pool_id: req.pool_id,
        retry: None,
        environment_id: req.environment_id,
    };

    dispatch_to_response(st.uc.execute(&req.project_id, req.case_ids, config).await)
}

fn dispatch_to_response(result: Result<crate::ports::DispatchReport, BatchRunError>) -> Response {
    match result {
        Ok(rep) => (
            StatusCode::OK,
            Json(BatchRunResponse { report_id: rep.report_id, status: rep.status }),
        )
            .into_response(),
        Err(BatchRunError::NoCases) => (StatusCode::BAD_REQUEST, "no cases to run").into_response(),
        Err(BatchRunError::InvalidRetryConfig) => {
            (StatusCode::BAD_REQUEST, "invalid retry config").into_response()
        }
        Err(BatchRunError::ResourcePoolNotConfigured) => (
            StatusCode::BAD_REQUEST,
            "resource pool not configured (supply poolId or set project default)",
        )
            .into_response(),
        Err(BatchRunError::ResourcePoolUnavailable { pool_id }) => {
            (StatusCode::CONFLICT, format!("resource pool unavailable: {pool_id}")).into_response()
        }
        Err(BatchRunError::Backend(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CaseRunRequest {
    project_id: String,
    #[serde(default = "default_serial")]
    run_mode: String,
    #[serde(default)]
    pool_id: Option<String>,
    #[serde(default)]
    environment_id: Option<String>,
}

fn default_serial() -> String {
    "SERIAL".to_string()
}

#[utoipa::path(
    post,
    path = "/api/case/{id}/run",
    tag = "api-test",
    params(("id" = String, Path)),
    request_body = CaseRunRequest,
    responses((status = 200, body = BatchRunResponse), (status = 400), (status = 403), (status = 409)),
    security(("bearer" = []))
)]
async fn run_case(
    user: AuthUser,
    State(st): State<BatchRunState>,
    Path(id): Path<String>,
    Json(req): Json<CaseRunRequest>,
) -> Response {
    if !user.can("API_DEFINITION", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let Some(mode) = BatchRunMode::parse(&req.run_mode) else {
        return (StatusCode::BAD_REQUEST, "unknown run mode").into_response();
    };
    let config = RunModeConfig {
        mode,
        pool_id: req.pool_id,
        retry: None,
        environment_id: req.environment_id,
    };
    dispatch_to_response(st.uc.execute(&req.project_id, vec![id], config).await)
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CaseExecutionRecordDto {
    report_id: String,
    case_id: String,
    outcome: String,
    failures: serde_json::Value,
    executed_at: String,
}

impl From<CaseExecutionRecord> for CaseExecutionRecordDto {
    fn from(r: CaseExecutionRecord) -> Self {
        Self {
            report_id: r.report_id,
            case_id: r.case_id,
            outcome: r.outcome,
            failures: r.failures,
            executed_at: r.executed_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CaseExecutionPageResponse {
    total: u64,
    current: u32,
    page_size: u32,
    total_pages: u64,
    items: Vec<CaseExecutionRecordDto>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct CaseExecutionQuery {
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

#[utoipa::path(
    get,
    path = "/api/case/{caseId}/executions",
    tag = "api-test",
    params(("caseId" = String, Path, description = "用例 id"), CaseExecutionQuery),
    responses((status = 200, body = CaseExecutionPageResponse), (status = 400), (status = 403)),
    security(("bearer" = []))
)]
async fn list_executions(
    user: AuthUser,
    State(st): State<ExecutionsState>,
    Path(case_id): Path<String>,
    Query(q): Query<CaseExecutionQuery>,
) -> Response {
    if !user.can("API_DEFINITION", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let page = match PageRequest::new(q.current, q.page_size) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid page params").into_response(),
    };
    match st.uc.execute(&case_id, page).await {
        Ok(page) => {
            let body = CaseExecutionPageResponse {
                total: page.total,
                current: page.current,
                page_size: page.page_size,
                total_pages: page.total_pages(),
                items: page.items.into_iter().map(CaseExecutionRecordDto::from).collect(),
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Clone)]
struct ResourcePoolState {
    create: CreateResourcePoolUseCase,
    list: ListResourcePoolsUseCase,
    edit: EditResourcePoolUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<ResourcePoolState> for Arc<dyn SessionStore> {
    fn from_ref(s: &ResourcePoolState) -> Self {
        s.sessions.clone()
    }
}

pub fn resource_pool_router(
    create: CreateResourcePoolUseCase,
    list: ListResourcePoolsUseCase,
    edit: EditResourcePoolUseCase,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    let state = ResourcePoolState { create, list, edit, sessions };
    Router::new()
        .route("/api/resource-pool", post(create_resource_pool).get(list_resource_pools))
        .route(
            "/api/resource-pool/{id}",
            get(get_resource_pool).put(update_resource_pool).delete(delete_resource_pool),
        )
        .with_state(state)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ResourcePoolBody {
    name: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    description: String,
    #[serde(default)]
    max_concurrency: i32,
    #[serde(default)]
    pool_type: String,
    #[serde(default = "default_true")]
    all_org: bool,
    #[serde(default)]
    org_ids: Vec<String>,
    #[serde(default)]
    server_url: String,
    #[serde(default)]
    config: serde_json::Value,
}

fn default_true() -> bool {
    true
}

impl ResourcePoolBody {
    fn into_draft(self) -> ResourcePoolDraft {
        ResourcePoolDraft {
            name: self.name,
            enabled: self.enabled,
            description: self.description,
            max_concurrency: self.max_concurrency,
            pool_type: self.pool_type,
            all_org: self.all_org,
            org_ids: self.org_ids,
            server_url: self.server_url,
            config: self.config,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ResourcePoolResponse {
    id: String,
    name: String,
    enabled: bool,
    description: String,
    max_concurrency: i32,
    pool_type: String,
    all_org: bool,
    org_ids: Vec<String>,
    server_url: String,
    #[schema(value_type = Object)]
    config: serde_json::Value,
    created_at: String,
    updated_at: String,
}

impl From<ResourcePool> for ResourcePoolResponse {
    fn from(p: ResourcePool) -> Self {
        Self {
            id: p.id,
            name: p.name,
            enabled: p.enabled,
            description: p.description,
            max_concurrency: p.max_concurrency,
            pool_type: p.pool_type,
            all_org: p.all_org,
            org_ids: p.org_ids,
            server_url: p.server_url,
            config: p.config,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[utoipa::path(post, path = "/api/resource-pool", tag = "api-test", request_body = ResourcePoolBody, responses((status = 201, body = ResourcePoolResponse), (status = 400), (status = 403), (status = 409)), security(("bearer" = [])))]
async fn create_resource_pool(
    user: AuthUser,
    State(st): State<ResourcePoolState>,
    Json(req): Json<ResourcePoolBody>,
) -> Response {
    if !user.can("RESOURCE_POOL", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.create.execute(req.into_draft()).await {
        Ok(p) => (StatusCode::CREATED, Json(ResourcePoolResponse::from(p))).into_response(),
        Err(CreateResourcePoolError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid resource pool payload").into_response()
        }
        Err(CreateResourcePoolError::Backend(PortError::Conflict(_))) => {
            (StatusCode::CONFLICT, "resource pool name already exists").into_response()
        }
        Err(CreateResourcePoolError::Backend(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/api/resource-pool", tag = "api-test", responses((status = 200, body = [ResourcePoolResponse]), (status = 403)), security(("bearer" = [])))]
async fn list_resource_pools(user: AuthUser, State(st): State<ResourcePoolState>) -> Response {
    if !user.can("RESOURCE_POOL", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.list.execute().await {
        Ok(list) => {
            let items: Vec<ResourcePoolResponse> =
                list.into_iter().map(ResourcePoolResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(get, path = "/api/resource-pool/{id}", tag = "api-test", responses((status = 200, body = ResourcePoolResponse), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn get_resource_pool(
    user: AuthUser,
    State(st): State<ResourcePoolState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("RESOURCE_POOL", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.edit.get(&id).await {
        Ok(Some(p)) => (StatusCode::OK, Json(ResourcePoolResponse::from(p))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "resource pool not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(put, path = "/api/resource-pool/{id}", tag = "api-test", request_body = ResourcePoolBody, responses((status = 200, body = ResourcePoolResponse), (status = 400), (status = 403), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn update_resource_pool(
    user: AuthUser,
    State(st): State<ResourcePoolState>,
    Path(id): Path<String>,
    Json(req): Json<ResourcePoolBody>,
) -> Response {
    if !user.can("RESOURCE_POOL", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.edit.update(&id, req.into_draft()).await {
        Ok(Some(p)) => (StatusCode::OK, Json(ResourcePoolResponse::from(p))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "resource pool not found").into_response(),
        Err(CreateResourcePoolError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid resource pool payload").into_response()
        }
        Err(CreateResourcePoolError::Backend(PortError::Conflict(_))) => {
            (StatusCode::CONFLICT, "resource pool name already exists").into_response()
        }
        Err(CreateResourcePoolError::Backend(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(delete, path = "/api/resource-pool/{id}", tag = "api-test", responses((status = 204), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn delete_resource_pool(
    user: AuthUser,
    State(st): State<ResourcePoolState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("RESOURCE_POOL", "DELETE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.edit.delete(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "resource pool not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        batch_run,
        run_case,
        list_executions,
        create_resource_pool,
        list_resource_pools,
        get_resource_pool,
        update_resource_pool,
        delete_resource_pool
    ),
    components(schemas(
        BatchRunRequest,
        CaseRunRequest,
        BatchRunResponse,
        CaseExecutionRecordDto,
        CaseExecutionPageResponse,
        ResourcePoolBody,
        ResourcePoolResponse
    )),
    tags((name = "api-test", description = "接口批量执行"))
)]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{FakeEnvironment, FakeResourcePool, SpyExecutor};
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn app(pools: FakeResourcePool) -> (Router, String) {
        let uc = StartBatchRunUseCase::new(
            Arc::new(pools),
            Arc::new(SpyExecutor::new()),
            Arc::new(FakeEnvironment::new()),
        );
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms =
            PermissionSet::from_raw(["API_DEFINITION:READ+EXECUTE".to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        (router(uc, sessions), token)
    }

    fn post(body: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder()
            .method("POST")
            .uri("/api/batch-run")
            .header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    #[tokio::test]
    async fn client_pool_available_returns_200_with_report() {
        let pools = FakeResourcePool::new().with_available("pool1");
        let (app, t) = app(pools).await;
        let resp = app
            .oneshot(post(
                r#"{"projectId":"p1","caseIds":["c1"],"runMode":"PARALLEL","poolId":"pool1"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(v["reportId"].as_str().expect("id").starts_with("report-"));
    }

    #[tokio::test]
    async fn no_pool_configured_returns_400() {
        let (app, t) = app(FakeResourcePool::new()).await;
        let resp = app
            .oneshot(post(r#"{"projectId":"p1","caseIds":["c1"],"runMode":"PARALLEL"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unavailable_pool_returns_409() {
        let (app, t) = app(FakeResourcePool::new()).await;
        let resp = app
            .oneshot(post(
                r#"{"projectId":"p1","caseIds":["c1"],"runMode":"PARALLEL","poolId":"dead"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn empty_cases_returns_400() {
        let pools = FakeResourcePool::new().with_available("pool1");
        let (app, t) = app(pools).await;
        let resp = app
            .oneshot(post(
                r#"{"projectId":"p1","caseIds":[],"runMode":"PARALLEL","poolId":"pool1"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn single_case_run_returns_200_with_report() {
        let pools = FakeResourcePool::new().with_available("pool1");
        let (app, t) = app(pools).await;
        let req = Request::builder()
            .method("POST")
            .uri("/api/case/c1/run")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {t}"))
            .body(Body::from(r#"{"projectId":"p1","poolId":"pool1"}"#.to_string()))
            .expect("req");
        let resp = app.oneshot(req).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(v["reportId"].as_str().expect("id").starts_with("report-"));
    }

    #[tokio::test]
    async fn unknown_run_mode_returns_400() {
        let pools = FakeResourcePool::new().with_available("pool1");
        let (app, t) = app(pools).await;
        let resp = app
            .oneshot(post(
                r#"{"projectId":"p1","caseIds":["c1"],"runMode":"WAT","poolId":"pool1"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    use crate::ports::{CaseExecutionQueryPort, CaseExecutionRecord, PortError};
    use async_trait::async_trait;

    struct FakeQuery {
        total: u64,
    }

    #[async_trait]
    impl CaseExecutionQueryPort for FakeQuery {
        async fn count_by_case(&self, _case_id: &str) -> Result<u64, PortError> {
            Ok(self.total)
        }
        async fn list_by_case(
            &self,
            case_id: &str,
            offset: u64,
            limit: u32,
        ) -> Result<Vec<CaseExecutionRecord>, PortError> {
            let end = (offset + limit as u64).min(self.total);
            Ok((offset..end)
                .map(|i| CaseExecutionRecord {
                    report_id: format!("r{i}"),
                    case_id: case_id.to_string(),
                    outcome: "SUCCESS".into(),
                    failures: serde_json::json!([]),
                    executed_at: "2026-05-31T00:00:00Z".into(),
                })
                .collect())
        }
    }

    async fn exec_app(total: u64) -> (Router, String) {
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms =
            PermissionSet::from_raw(["API_DEFINITION:READ+EXECUTE".to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        let r = executions_router(
            ListCaseExecutionsUseCase::new(Arc::new(FakeQuery { total })),
            sessions,
        );
        (r, token)
    }

    fn exec_get(uri: &str, token: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .expect("req")
    }

    #[tokio::test]
    async fn executions_returns_paginated_body() {
        let (app, t) = exec_app(3).await;
        let resp = app
            .oneshot(exec_get("/api/case/c1/executions?current=1&pageSize=2", &t))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["total"], 3);
        assert_eq!(v["current"], 1);
        assert_eq!(v["pageSize"], 2);
        assert_eq!(v["totalPages"], 2);
        let items = v["items"].as_array().expect("arr");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["reportId"], "r0");
        assert_eq!(items[0]["caseId"], "c1");
        assert_eq!(items[0]["executedAt"], "2026-05-31T00:00:00Z");
        assert!(items[0]["failures"].is_array());
    }

    #[tokio::test]
    async fn executions_defaults_apply_without_query() {
        let (app, t) = exec_app(0).await;
        let resp = app.oneshot(exec_get("/api/case/c1/executions", &t)).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["current"], 1);
        assert_eq!(v["pageSize"], 10);
    }

    #[tokio::test]
    async fn executions_bad_page_params_returns_400() {
        let (app, t) = exec_app(3).await;
        let resp = app
            .oneshot(exec_get("/api/case/c1/executions?current=0&pageSize=10", &t))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    use crate::domain::{NewResourcePool, ResourcePool};
    use crate::ports::ResourcePoolAdminPort;
    use kernel::permission::PermissionSet;
    use std::sync::Mutex;
    use webauth::testing::InMemorySessionStore;

    #[derive(Default)]
    struct FakePoolAdmin {
        pools: Mutex<Vec<ResourcePool>>,
    }

    fn pool_view(id: String, p: &NewResourcePool) -> ResourcePool {
        ResourcePool {
            id,
            name: p.name.clone(),
            enabled: p.enabled,
            description: p.description.clone(),
            max_concurrency: p.max_concurrency,
            pool_type: p.pool_type.clone(),
            all_org: p.all_org,
            org_ids: p.org_ids.clone(),
            server_url: p.server_url.clone(),
            config: p.config.clone(),
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
        }
    }

    #[async_trait]
    impl ResourcePoolAdminPort for FakePoolAdmin {
        async fn create(&self, p: &NewResourcePool) -> Result<ResourcePool, PortError> {
            let mut g = self.pools.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let view = pool_view(format!("p{}", g.len() + 1), p);
            g.push(view.clone());
            Ok(view)
        }
        async fn list(&self) -> Result<Vec<ResourcePool>, PortError> {
            Ok(self.pools.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone())
        }
        async fn get(&self, id: &str) -> Result<Option<ResourcePool>, PortError> {
            Ok(self
                .pools
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .find(|p| p.id == id)
                .cloned())
        }
        async fn update(
            &self,
            id: &str,
            p: &NewResourcePool,
        ) -> Result<Option<ResourcePool>, PortError> {
            let mut g = self.pools.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            match g.iter_mut().find(|x| x.id == id) {
                Some(slot) => {
                    *slot = pool_view(id.to_string(), p);
                    Ok(Some(slot.clone()))
                }
                None => Ok(None),
            }
        }
        async fn delete(&self, id: &str) -> Result<bool, PortError> {
            let mut g = self.pools.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let before = g.len();
            g.retain(|x| x.id != id);
            Ok(g.len() != before)
        }
    }

    async fn pool_app() -> (Router, String) {
        let admin = Arc::new(FakePoolAdmin::default());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["RESOURCE_POOL:READ+ADD+UPDATE+DELETE".to_string()])
            .expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        let r = resource_pool_router(
            CreateResourcePoolUseCase::new(admin.clone()),
            ListResourcePoolsUseCase::new(admin.clone()),
            EditResourcePoolUseCase::new(admin),
            sessions,
        );
        (r, token)
    }

    fn pool_post(body: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder()
            .method("POST")
            .uri("/api/resource-pool")
            .header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    #[tokio::test]
    async fn create_pool_with_permission_201() {
        let (app, t) = pool_app().await;
        let resp = app.oneshot(pool_post(r#"{"name":"本地池"}"#, Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["name"], "本地池");
        assert_eq!(v["enabled"], true);
        assert!(v["id"].as_str().expect("id").starts_with('p'));
    }

    #[tokio::test]
    async fn create_pool_without_token_401() {
        let (app, _t) = pool_app().await;
        let resp = app.oneshot(pool_post(r#"{"name":"x"}"#, None)).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_pool_without_permission_403() {
        let admin = Arc::new(FakePoolAdmin::default());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["RESOURCE_POOL:READ".to_string()]).expect("perms");
        let token = sessions.create("viewer", perms, 3600).await.expect("token");
        let app = resource_pool_router(
            CreateResourcePoolUseCase::new(admin.clone()),
            ListResourcePoolsUseCase::new(admin.clone()),
            EditResourcePoolUseCase::new(admin),
            sessions,
        );
        let resp = app.oneshot(pool_post(r#"{"name":"x"}"#, Some(&token))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn create_pool_blank_name_400() {
        let (app, t) = pool_app().await;
        let resp = app.oneshot(pool_post(r#"{"name":"   "}"#, Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_pools_with_token_200() {
        let (app, t) = pool_app().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/resource-pool")
                    .header("authorization", format!("Bearer {t}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn list_pools_without_token_401() {
        let (app, _t) = pool_app().await;
        let resp = app
            .oneshot(Request::builder().uri("/api/resource-pool").body(Body::empty()).expect("req"))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    async fn json_of(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    #[tokio::test]
    async fn create_carries_extended_fields() {
        let (app, t) = pool_app().await;
        let body = r#"{"name":"k8s池","poolType":"Kubernetes","maxConcurrency":10,"description":"d","allOrg":false,"orgIds":["org-1"],"serverUrl":"http://ms","config":{"namespace":"ns"}}"#;
        let resp = app.oneshot(pool_post(body, Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = json_of(resp).await;
        assert_eq!(v["poolType"], "Kubernetes");
        assert_eq!(v["maxConcurrency"], 10);
        assert_eq!(v["allOrg"], false);
        assert_eq!(v["orgIds"][0], "org-1");
        assert_eq!(v["config"]["namespace"], "ns");
        assert!(v["createdAt"].as_str().expect("createdAt").len() >= 10);
    }

    #[tokio::test]
    async fn update_then_delete_roundtrip() {
        let (app, t) = pool_app().await;
        let created = json_of(
            app.clone().oneshot(pool_post(r#"{"name":"池"}"#, Some(&t))).await.expect("resp"),
        )
        .await;
        let id = created["id"].as_str().expect("id").to_string();

        let put = Request::builder()
            .method("PUT")
            .uri(format!("/api/resource-pool/{id}"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {t}"))
            .body(Body::from(r#"{"name":"改名","enabled":false}"#))
            .expect("req");
        let resp = app.clone().oneshot(put).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_of(resp).await;
        assert_eq!(v["name"], "改名");
        assert_eq!(v["enabled"], false);

        let del = Request::builder()
            .method("DELETE")
            .uri(format!("/api/resource-pool/{id}"))
            .header("authorization", format!("Bearer {t}"))
            .body(Body::empty())
            .expect("req");
        let resp = app.clone().oneshot(del).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let del2 = Request::builder()
            .method("DELETE")
            .uri(format!("/api/resource-pool/{id}"))
            .header("authorization", format!("Bearer {t}"))
            .body(Body::empty())
            .expect("req");
        let resp = app.oneshot(del2).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
