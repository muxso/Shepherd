//! 组装根桥:`POST /perf/run` + `GET /perf/report/{id}`。
//!
//! 把 `perf` 原生压测引擎接进服务:校验负载计划 → 落 RUNNING 报告 → **后台并发施压**
//! (perf::run_load + ApiRunnerExecutor,复用 reqwest,不经 JMeter)→ 跑完回写聚合指标。
//! 与 batch-run 一样属"接口测试"邻域;施压是写动作,RBAC 资源键 `PERF:EXECUTE`,读开放。
//! SQL 收敛在 perf 的 PG 适配器(PgPerfReportStore),组装根不碰裸 sqlx。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use migrate::PgPool;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use api_runner::{Assertion, HttpMethod, RequestSpec};
use perf::adapters::{run_load, ApiRunnerExecutor, PgPerfReportStore};
use perf::domain::LoadPlan;

#[derive(Clone)]
struct PerfState {
    store: PgPerfReportStore,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<PerfState> for Arc<dyn SessionStore> {
    fn from_ref(s: &PerfState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(pool: PgPool, sessions: Arc<dyn SessionStore>) -> Router {
    let store = PgPerfReportStore::new(pool);
    Router::new()
        .route("/perf/run", post(run_perf))
        .route("/perf/report/{id}", get(get_report))
        .with_state(PerfState { store, sessions })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunPerfBody {
    #[serde(default)]
    project_id: String,
    #[serde(default = "default_method")]
    method: String,
    url: String,
    #[serde(default = "default_concurrency")]
    concurrency: usize,
    #[serde(default = "default_iterations")]
    iterations: usize,
    /// 期望状态码:给定则成功=该码命中;省略则成功=HTTP 可达。
    #[serde(default)]
    expect_status: Option<u16>,
}

fn default_method() -> String {
    "GET".to_string()
}
fn default_concurrency() -> usize {
    10
}
fn default_iterations() -> usize {
    100
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunPerfResponse {
    report_id: String,
    status: String,
}

fn method_of(m: &str) -> HttpMethod {
    serde_json::from_value(serde_json::Value::String(m.to_uppercase())).unwrap_or(HttpMethod::Get)
}

#[utoipa::path(
    post, path = "/perf/run", tag = "perf",
    request_body = RunPerfBody,
    responses((status = 200, body = RunPerfResponse), (status = 400), (status = 403)),
    security(("bearer" = []))
)]
async fn run_perf(user: AuthUser, State(st): State<PerfState>, Json(req): Json<RunPerfBody>) -> Response {
    if !user.can("PERF", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    if req.url.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "url required").into_response();
    }
    let plan = match LoadPlan::new(req.concurrency, req.iterations) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("invalid load plan: {e}")).into_response(),
    };

    // 1) 落 RUNNING 报告
    let report_id = match st
        .store
        .create(&req.project_id, &req.method.to_uppercase(), &req.url, req.concurrency as i32, req.iterations as i32)
        .await
    {
        Ok(id) => id,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "create report error").into_response(),
    };

    // 2) 后台并发施压,跑完回写聚合指标(不阻塞响应)
    let spec = RequestSpec {
        method: method_of(&req.method),
        url: req.url.clone(),
        headers: vec![],
        body: None,
    };
    let assertions: Vec<Assertion> =
        req.expect_status.map(|c| vec![Assertion::StatusIs(c)]).unwrap_or_default();
    let store = st.store.clone();
    let id = report_id.clone();
    tokio::spawn(async move {
        let exec = Arc::new(ApiRunnerExecutor::new(spec, assertions));
        let report = run_load(&plan, exec).await;
        let _ = store.finish(&id, &report).await;
    });

    (StatusCode::OK, Json(RunPerfResponse { report_id, status: "RUNNING".to_string() })).into_response()
}

#[utoipa::path(
    get, path = "/perf/report/{id}", tag = "perf",
    params(("id" = String, Path)),
    responses((status = 200), (status = 404))
)]
async fn get_report(State(st): State<PerfState>, Path(id): Path<String>) -> Response {
    match st.store.get_json(&id).await {
        Ok(Some(body)) => (StatusCode::OK, Json(body)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "perf report not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(run_perf, get_report),
    components(schemas(RunPerfBody, RunPerfResponse)),
    tags((name = "perf", description = "原生压测"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
