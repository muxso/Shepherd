//! 组装根桥:`POST /perf/run` + `GET /perf/report/{id}`。
//!
//! 把 `perf` 原生压测引擎接进服务:校验负载计划 → 落 RUNNING 报告 → **后台并发施压**
//! (perf::run_load + ApiRunnerExecutor,复用 reqwest,不经 JMeter)→ 跑完回写聚合指标。
//! 与 batch-run 一样属"接口测试"邻域;施压是写动作,RBAC 资源键 `PERF:EXECUTE`,读开放。
//! SQL 收敛在 perf 的 PG 适配器(PgPerfReportStore),组装根不碰裸 sqlx。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use migrate::PgPool;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use api_scenario::application::{CompileError, CompileScenarioUseCase};
use api_scenario::ports::ApiScenarioRepository;
use api_test::adapters::local::{CaseResultSink, CaseRunSpec, CaseSpecSource};
use api_test::adapters::plan::{PlanExecutor, PlanNode};
use api_test::domain::ResolvedEnv;
use api_test::ports::{EnvironmentPort, PortError};
use perf::adapters::{run_collect, ParquetObjectStoreSink, PgPerfReportStore, ProbeExecutor};
use perf::domain::LoadPlan;
use perf::ports::{RequestExecutor, SampleSink};
use probe::{ProbeAssertion, ProbeRequest};

use crate::scenario_run::{count_leaves, to_nodes};

#[derive(Clone)]
struct PerfState {
    store: PgPerfReportStore,
    /// 原始样本下沉(Parquet+对象存储);未配置 PERF_SAMPLES_PATH 则为 None,仅存聚合。
    sink: Option<Arc<dyn SampleSink>>,
    sessions: Arc<dyn SessionStore>,
    // —— 场景压测依赖(编译场景成计划树 + 取用例规格 + 解析环境)——
    compile: CompileScenarioUseCase,
    scenario_repo: Arc<dyn ApiScenarioRepository>,
    case_specs: Arc<dyn CaseSpecSource>,
    envs: Arc<dyn EnvironmentPort>,
}

impl FromRef<PerfState> for Arc<dyn SessionStore> {
    fn from_ref(s: &PerfState) -> Self {
        s.sessions.clone()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn router(
    pool: PgPool,
    sessions: Arc<dyn SessionStore>,
    compile: CompileScenarioUseCase,
    scenario_repo: Arc<dyn ApiScenarioRepository>,
    case_specs: Arc<dyn CaseSpecSource>,
    envs: Arc<dyn EnvironmentPort>,
) -> Router {
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
        .route("/perf/scenario/run", post(run_scenario_perf))
        .route("/perf/report/{id}", get(get_report))
        .with_state(PerfState { store, sink, sessions, compile, scenario_repo, case_specs, envs })
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

// ============================ 场景压测 ============================
//
// 与单接口压测共用同一台引擎(perf::run_collect)与报告表(ms_perf_report),区别只在
// 「一次施压 = 跑完整条场景链」:把场景编译成计划树,每个虚拟用户每轮迭代顺序跑整棵树
// (登录 → 提取 token → 鉴权调用 → …),墙钟测端到端延迟,全部步通过才记成功。
// 报告 method=SCENARIO、url=场景名,前端复用既有 PerfReport 视图。

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunScenarioPerfBody {
    #[serde(default)]
    project_id: String,
    /// 被压测的场景 id。
    scenario_id: String,
    /// 运行环境 id(注入 base_url/默认头/变量种子);缺省则空环境。
    #[serde(default)]
    environment_id: Option<String>,
    #[serde(default = "default_concurrency")]
    concurrency: usize,
    #[serde(default = "default_iterations")]
    iterations: usize,
    /// 时长模式:给定则持续压测该毫秒数(忽略 iterations)。
    #[serde(default)]
    duration_ms: Option<u64>,
}

/// 计划树结果汇:压测里不落库(每轮迭代都会跑一遍,逐步明细无意义且会拖垮 DB)。
/// 全部方法 no-op;成功/失败由 `PlanExecutor::run` 的返回值决定。
struct NoopSink;

#[async_trait]
impl CaseResultSink for NoopSink {
    async fn record(
        &self,
        _report_id: &str,
        _case_id: &str,
        _outcome: &str,
        _failures: &[String],
    ) -> Result<(), PortError> {
        Ok(())
    }
}

/// 用例规格缓存:同一轮压测里每个 case 只查一次 DB,之后命中内存。
/// 关键 —— 否则高并发 × 高迭代会把「被测目标的压测」变成「自家 PG 的压测」,延迟全是查规格的开销。
struct CachingSpecSource {
    inner: Arc<dyn CaseSpecSource>,
    cache: RwLock<HashMap<String, Option<CaseRunSpec>>>,
}

impl CachingSpecSource {
    fn new(inner: Arc<dyn CaseSpecSource>) -> Self {
        Self { inner, cache: RwLock::new(HashMap::new()) }
    }
}

#[async_trait]
impl CaseSpecSource for CachingSpecSource {
    async fn spec_of(&self, case_id: &str) -> Result<Option<CaseRunSpec>, PortError> {
        if let Some(hit) = self.cache.read().await.get(case_id).cloned() {
            return Ok(hit);
        }
        let fetched = self.inner.spec_of(case_id).await?;
        self.cache.write().await.insert(case_id.to_string(), fetched.clone());
        Ok(fetched)
    }
}

/// 把「跑一整棵计划树」接成 perf 引擎的单次施压单元:每次 `execute` 顺序走完整条场景链,
/// 引擎用墙钟测端到端延迟。失败策略不停(`stop_on_failure=false`)→ 跑满所有步;
/// 成功 = 全部叶子通过。`report_id` 占位(NoopSink 不落库)。变量种子每轮都从环境复制,
/// 各虚拟用户互不串扰(`PlanExecutor::run` 内部各持一份 RunState)。
struct ScenarioExecutor {
    exec: PlanExecutor,
    nodes: Vec<PlanNode>,
    env: ResolvedEnv,
}

#[async_trait]
impl RequestExecutor for ScenarioExecutor {
    async fn execute(&self) -> bool {
        self.exec.run("perf", &self.nodes, &self.env, false).await.unwrap_or(false)
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunScenarioPerfResponse {
    report_id: String,
    status: String,
    /// 场景静态叶子数(供前端提示「每轮 N 步」)。
    step_count: usize,
}

#[utoipa::path(
    post, path = "/perf/scenario/run", tag = "perf",
    request_body = RunScenarioPerfBody,
    responses((status = 200, body = RunScenarioPerfResponse), (status = 400), (status = 403), (status = 404), (status = 409)),
    security(("bearer" = []))
)]
async fn run_scenario_perf(
    user: AuthUser,
    State(st): State<PerfState>,
    Json(req): Json<RunScenarioPerfBody>,
) -> Response {
    if !user.can("PERF", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    if req.scenario_id.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "scenarioId required").into_response();
    }

    // 1) 编译场景为计划树(复用场景串行执行那套中立树 → 执行器节点转换)
    let plan = match st.compile.compile_plan(&req.scenario_id).await {
        Ok(p) => p,
        Err(CompileError::NotFound(_)) => {
            return (StatusCode::NOT_FOUND, "scenario not found").into_response();
        }
        Err(CompileError::Cycle(_)) | Err(CompileError::Depth(_)) => {
            return (StatusCode::CONFLICT, "scenario reference cycle or too deep").into_response();
        }
        Err(CompileError::Repo(_)) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
        }
    };
    let mut once = 0u32;
    let nodes = to_nodes(&plan, &mut once);
    let step_count = count_leaves(&nodes);
    if step_count == 0 {
        return (StatusCode::BAD_REQUEST, "scenario has no runnable steps").into_response();
    }

    // 2) 负载计划(时长模式优先)
    let plan_spec = match req.duration_ms {
        Some(ms) => LoadPlan::duration_ms(req.concurrency, ms),
        None => LoadPlan::new(req.concurrency, req.iterations),
    };
    let plan_spec = match plan_spec {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("invalid load plan: {e}")).into_response()
        }
    };
    let planned_iterations = if req.duration_ms.is_some() { 0 } else { req.iterations as i32 };

    // 3) 解析运行环境(给定 environmentId 才注入)
    let env = match req.environment_id.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(eid) => match st.envs.resolve(eid).await {
            Ok(e) => e.unwrap_or_default(),
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "env resolve error").into_response()
            }
        },
        None => ResolvedEnv::default(),
    };

    // 4) 报告标识:method=SCENARIO、url=场景名(取不到回落 id)
    let scenario_name = match st.scenario_repo.get_scenario(&req.scenario_id).await {
        Ok(Some(s)) => s.name,
        _ => req.scenario_id.clone(),
    };

    // 5) 组装执行器:计划树执行器配 NoopSink(不落逐步明细)+ 规格缓存(每 case 只查一次)
    let plan_exec = PlanExecutor::new(
        Arc::new(CachingSpecSource::new(st.case_specs.clone())),
        Arc::new(NoopSink),
    );
    let executor: Arc<dyn RequestExecutor> =
        Arc::new(ScenarioExecutor { exec: plan_exec, nodes, env });

    // 6) 落 RUNNING 报告
    let report_id = match st
        .store
        .create(&req.project_id, "SCENARIO", &scenario_name, req.concurrency as i32, planned_iterations)
        .await
    {
        Ok(id) => id,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "create report error").into_response(),
    };

    // 7) 后台并发施压,跑完回写聚合指标(与单接口压测同路径)
    let store = st.store.clone();
    let sink = st.sink.clone();
    let id = report_id.clone();
    tokio::spawn(async move {
        let (report, samples) = run_collect(&plan_spec, executor).await;
        let samples_key: Option<String> = match &sink {
            Some(s) => match s.write(&id, &samples).await {
                Ok(key) => Some(key),
                Err(e) => {
                    tracing::warn!(report = %id, "perf 场景样本下沉失败: {e:?}");
                    None
                }
            },
            None => None,
        };
        let _ = store.finish(&id, &report, samples_key.as_deref()).await;
    });

    (
        StatusCode::OK,
        Json(RunScenarioPerfResponse { report_id, status: "RUNNING".to_string(), step_count }),
    )
        .into_response()
}

#[derive(OpenApi)]
#[openapi(
    paths(run_perf, run_scenario_perf, get_report),
    components(schemas(RunPerfBody, RunPerfResponse, RunScenarioPerfBody, RunScenarioPerfResponse)),
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
