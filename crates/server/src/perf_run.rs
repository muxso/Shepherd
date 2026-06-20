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
use perf::adapters::{
    run_collect, ApiRunnerExecutor, ParquetObjectStoreSink, PgPerfReportStore, SqlExecutor,
};
use perf::domain::LoadPlan;
use perf::ports::{RequestExecutor, SampleSink};

#[derive(Clone)]
struct PerfState {
    store: PgPerfReportStore,
    /// 原始样本下沉(Parquet+对象存储);未配置 PERF_SAMPLES_PATH 则为 None,仅存聚合。
    sink: Option<Arc<dyn SampleSink>>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<PerfState> for Arc<dyn SessionStore> {
    fn from_ref(s: &PerfState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(pool: PgPool, sessions: Arc<dyn SessionStore>) -> Router {
    let store = PgPerfReportStore::new(pool);
    // 配置了 PERF_SAMPLES_PATH 才下沉原始样本(本地 object_store 后端;生产可换 S3)。
    // 构造失败则降级为不下沉(仅聚合),不影响压测可用。
    let sink: Option<Arc<dyn SampleSink>> = std::env::var("PERF_SAMPLES_PATH")
        .ok()
        .filter(|p| !p.trim().is_empty())
        .and_then(|root| {
            if let Err(e) = std::fs::create_dir_all(&root) {
                tracing::warn!("PERF_SAMPLES_PATH 不可建({root}): {e};样本下沉关闭");
                return None;
            }
            match ParquetObjectStoreSink::new_local(&root, "perf") {
                Ok(s) => {
                    tracing::info!("perf 样本下沉已启用 → {root}");
                    Some(Arc::new(s) as Arc<dyn SampleSink>)
                }
                Err(e) => {
                    tracing::warn!("perf 样本下沉初始化失败: {e:?};仅存聚合");
                    None
                }
            }
        });
    Router::new()
        .route("/perf/run", post(run_perf))
        .route("/perf/report/{id}", get(get_report))
        .with_state(PerfState { store, sink, sessions })
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
    /// 时长模式:给定则持续压测该毫秒数(忽略 iterations);省略则按 iterations 固定次数。
    #[serde(default)]
    duration_ms: Option<u64>,
    /// 期望状态码:给定则成功=该码命中;省略则成功=HTTP 可达。
    #[serde(default)]
    expect_status: Option<u16>,
    /// 协议:HTTP(默认)| SQL。SQL 时 url 为连接串、query 为待压测语句。
    #[serde(default = "default_protocol")]
    protocol: String,
    /// SQL 协议待压测的语句(默认 SELECT 1)。
    #[serde(default)]
    query: Option<String>,
}

fn default_protocol() -> String {
    "HTTP".to_string()
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
    // 时长模式优先;否则固定次数。
    let plan = match req.duration_ms {
        Some(ms) => LoadPlan::duration_ms(req.concurrency, ms),
        None => LoadPlan::new(req.concurrency, req.iterations),
    };
    let plan = match plan {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("invalid load plan: {e}")).into_response(),
    };
    // 报告里记录的 iterations:时长模式记 0(实际完成数见 total)。
    let planned_iterations = if req.duration_ms.is_some() { 0 } else { req.iterations as i32 };

    // 1) 据协议构建执行器(SQL 连接失败提前返回)。
    //    报告记录:HTTP 记 method+url;SQL 记 method=SQL、url=语句(不存连接串,避免泄漏口令)。
    let (executor, report_method, report_url): (Arc<dyn RequestExecutor>, String, String) =
        if req.protocol.eq_ignore_ascii_case("SQL") {
            let query = req.query.clone().unwrap_or_else(|| "SELECT 1".to_string());
            match SqlExecutor::connect(&req.url, &query, req.concurrency as u32).await {
                Ok(e) => (Arc::new(e), "SQL".to_string(), query),
                Err(e) => {
                    return (StatusCode::BAD_GATEWAY, format!("sql connect failed: {e}"))
                        .into_response()
                }
            }
        } else {
            let spec = RequestSpec {
                method: method_of(&req.method),
                url: req.url.clone(),
                headers: vec![],
                body: None,
            };
            let assertions: Vec<Assertion> =
                req.expect_status.map(|c| vec![Assertion::StatusIs(c)]).unwrap_or_default();
            (Arc::new(ApiRunnerExecutor::new(spec, assertions)), req.method.to_uppercase(), req.url.clone())
        };

    // 2) 落 RUNNING 报告
    let report_id = match st
        .store
        .create(&req.project_id, &report_method, &report_url, req.concurrency as i32, planned_iterations)
        .await
    {
        Ok(id) => id,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "create report error").into_response(),
    };

    // 3) 后台并发施压,跑完回写聚合指标(不阻塞响应)
    let store = st.store.clone();
    let sink = st.sink.clone();
    let id = report_id.clone();
    tokio::spawn(async move {
        // run_collect 同时拿到聚合报告与原始逐请求样本。
        let (report, samples) = run_collect(&plan, executor).await;
        // 原始样本下沉对象存储(配置了 sink 才做);成功则把存储键随报告落库。
        let samples_key: Option<String> = match &sink {
            Some(s) => match s.write(&id, &samples).await {
                Ok(key) => Some(key),
                Err(e) => {
                    tracing::warn!(report = %id, "perf 样本下沉失败: {e:?}");
                    None
                }
            },
            None => None,
        };
        let _ = store.finish(&id, &report, samples_key.as_deref()).await;
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
