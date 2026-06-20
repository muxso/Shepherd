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

use perf::adapters::{run_collect, ParquetObjectStoreSink, PgPerfReportStore, ProbeExecutor};
use perf::domain::LoadPlan;
use perf::ports::{RequestExecutor, SampleSink};
use probe::{ProbeAssertion, ProbeRequest};

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
    /// 期望状态码:给定 → StatusIs 断言(HTTP=状态码;其它协议 OK 时 status=0)。
    #[serde(default)]
    expect_status: Option<u16>,
    /// 断言:输出包含子串(OutputContains)。
    #[serde(default)]
    expect_contains: Option<String>,
    /// 断言:输出等于(OutputEquals)。
    #[serde(default)]
    expect_equals: Option<String>,
    /// 断言:单次延迟不超过该毫秒数(LatencyUnderMs)。
    #[serde(default)]
    latency_under_ms: Option<u64>,
    /// 协议(经 probe 注册表):HTTP(默认)| SQL | GRPC | REDIS | MYSQL | WEBSOCKET | …(取决于启用的插件)。
    /// 非 HTTP 时 url 为目标(连接串/端点),query 为载荷(SQL=语句、GRPC=方法路径、REDIS=命令、WS=消息)。
    #[serde(default = "default_protocol")]
    protocol: String,
    /// 协议载荷:SQL=语句(默认 SELECT 1)、GRPC=方法路径、REDIS=命令(默认 PING)、WS=消息。
    #[serde(default)]
    query: Option<String>,
}

/// 把请求里的断言字段统一映射成 probe 断言(对**任意协议**生效)。
fn build_assertions(req: &RunPerfBody) -> Vec<ProbeAssertion> {
    let mut a = Vec::new();
    if let Some(c) = req.expect_status {
        a.push(ProbeAssertion::StatusIs(c as i64));
    }
    if let Some(s) = req.expect_contains.clone() {
        a.push(ProbeAssertion::OutputContains(s));
    }
    if let Some(s) = req.expect_equals.clone() {
        a.push(ProbeAssertion::OutputEquals(s));
    }
    if let Some(ms) = req.latency_under_ms {
        a.push(ProbeAssertion::LatencyUnderMs(ms));
    }
    a
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

    // 1) 据协议构建**统一执行器**:协议无关的 ProbeRequest → ProbeExecutor(经 probe 注册表分发)。
    //    加协议 = 加 probe 插件,这里无需再改。报告记录:HTTP 记 method+url;
    //    SQL 记 method=SQL、url=语句(不存连接串,避免泄漏口令);GRPC 记 method=GRPC、url=方法路径。
    //    连接惰性建立并由插件按 target 缓存(压测复用连接);grpc 缺方法路径是纯校验,提前 400。
    //    断言(expect_status/contains/equals/latency)对**任意协议**统一生效。
    let assertions = build_assertions(&req);
    let proto = req.protocol.to_lowercase();
    let (probe_req, report_method, report_url): (ProbeRequest, String, String) = if proto == "sql" {
        let query = req.query.clone().unwrap_or_else(|| "SELECT 1".to_string());
        (
            ProbeRequest {
                protocol: "sql".to_string(),
                target: req.url.clone(),
                payload: Some(query.clone()),
                metadata: std::collections::BTreeMap::new(),
                assertions,
            },
            "SQL".to_string(),
            query,
        )
    } else if proto == "grpc" {
        let method = match req.query.clone().filter(|q| !q.trim().is_empty()) {
            Some(m) => m,
            None => {
                return (StatusCode::BAD_REQUEST, "grpc requires query=method path").into_response()
            }
        };
        let mut metadata = std::collections::BTreeMap::new();
        metadata.insert("method".to_string(), method.clone());
        (
            ProbeRequest {
                protocol: "grpc".to_string(),
                target: req.url.clone(),
                payload: None,
                metadata,
                assertions,
            },
            "GRPC".to_string(),
            method,
        )
    } else if proto == "http" {
        let mut metadata = std::collections::BTreeMap::new();
        metadata.insert("method".to_string(), req.method.to_uppercase());
        // 无断言时:成功=HTTP 可达(传输成功);有 expect_* 则按断言判定。
        (
            ProbeRequest {
                protocol: "http".to_string(),
                target: req.url.clone(),
                payload: None,
                metadata,
                assertions,
            },
            req.method.to_uppercase(),
            req.url.clone(),
        )
    } else {
        // 通用协议(redis/mysql/websocket 及未来插件):url=目标,query=载荷。
        // 加协议无需改这里 —— 只要 probe 有对应插件即可;断言同样生效。
        let payload = req.query.clone();
        (
            ProbeRequest {
                protocol: proto.clone(),
                target: req.url.clone(),
                payload: payload.clone(),
                metadata: std::collections::BTreeMap::new(),
                assertions,
            },
            proto.to_uppercase(),
            payload.unwrap_or_else(|| req.url.clone()),
        )
    };
    // 整轮压测共享一个注册表实例:插件内部按 target 缓存连接,worker 间复用;本轮结束随之释放。
    let registry = Arc::new(probe::default_registry());
    let executor: Arc<dyn RequestExecutor> = Arc::new(ProbeExecutor::new(registry, probe_req));

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn body(v: serde_json::Value) -> RunPerfBody {
        serde_json::from_value(v).expect("body")
    }

    #[test]
    fn no_assertion_fields_yields_empty() {
        // 无 expect_* → 空断言(成功=传输可达)。
        assert!(build_assertions(&body(json!({"url": "http://x"}))).is_empty());
    }

    #[test]
    fn maps_all_assertion_fields() {
        let a = build_assertions(&body(json!({
            "url": "http://x",
            "expectStatus": 200,
            "expectContains": "ok",
            "expectEquals": "PONG",
            "latencyUnderMs": 500
        })));
        assert_eq!(a.len(), 4);
        assert!(a.contains(&ProbeAssertion::StatusIs(200)));
        assert!(a.contains(&ProbeAssertion::OutputContains("ok".into())));
        assert!(a.contains(&ProbeAssertion::OutputEquals("PONG".into())));
        assert!(a.contains(&ProbeAssertion::LatencyUnderMs(500)));
    }

    #[test]
    fn maps_subset() {
        // 只给 contains → 只产出 OutputContains(redis/ws/mysql 压测带断言的常见用法)。
        let a = build_assertions(&body(json!({"url": "redis://x", "expectContains": "hello"})));
        assert_eq!(a, vec![ProbeAssertion::OutputContains("hello".into())]);
    }
}
