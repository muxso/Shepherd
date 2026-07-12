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
    sink: Option<Arc<dyn SampleSink>>,
    sessions: Arc<dyn SessionStore>,
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
    let sink: Option<Arc<dyn SampleSink>> =
        std::env::var("PERF_SAMPLES_PATH").ok().filter(|p| !p.trim().is_empty()).and_then(|root| {
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
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    expect_status: Option<u16>,
    #[serde(default)]
    expect_contains: Option<String>,
    #[serde(default)]
    expect_equals: Option<String>,
    #[serde(default)]
    latency_under_ms: Option<u64>,
    #[serde(default = "default_protocol")]
    protocol: String,
    #[serde(default)]
    query: Option<String>,
}

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
async fn run_perf(
    user: AuthUser,
    State(st): State<PerfState>,
    Json(req): Json<RunPerfBody>,
) -> Response {
    if !user.can("PERF", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    if req.url.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "url required").into_response();
    }
    let plan = match req.duration_ms {
        Some(ms) => LoadPlan::duration_ms(req.concurrency, ms),
        None => LoadPlan::new(req.concurrency, req.iterations),
    };
    let plan = match plan {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("invalid load plan: {e}")).into_response()
        }
    };
    // Duration mode records 0 (actual completions are in `total`); otherwise use iterations.
    let planned_iterations = if req.duration_ms.is_some() { 0 } else { req.iterations as i32 };

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
    let registry = Arc::new(probe::default_registry());
    let executor: Arc<dyn RequestExecutor> = Arc::new(ProbeExecutor::new(registry, probe_req));

    let report_id = match st
        .store
        .create(
            &req.project_id,
            &report_method,
            &report_url,
            req.concurrency as i32,
            planned_iterations,
        )
        .await
    {
        Ok(id) => id,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "create report error").into_response()
        }
    };

    let store = st.store.clone();
    let sink = st.sink.clone();
    let id = report_id.clone();
    tokio::spawn(async move {
        let (report, samples) = run_collect(&plan, executor).await;
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

    (StatusCode::OK, Json(RunPerfResponse { report_id, status: "RUNNING".to_string() }))
        .into_response()
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

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunScenarioPerfBody {
    #[serde(default)]
    project_id: String,
    scenario_id: String,
    #[serde(default)]
    environment_id: Option<String>,
    #[serde(default = "default_concurrency")]
    concurrency: usize,
    #[serde(default = "default_iterations")]
    iterations: usize,
    #[serde(default)]
    duration_ms: Option<u64>,
}

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

/// Query the DB once per case; otherwise high concurrency x high iterations turns
/// the load test into a load test of our own PG.
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

    let env = match req.environment_id.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(eid) => match st.envs.resolve(eid).await {
            Ok(e) => e.unwrap_or_default(),
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "env resolve error").into_response()
            }
        },
        None => ResolvedEnv::default(),
    };

    let scenario_name = match st.scenario_repo.get_scenario(&req.scenario_id).await {
        Ok(Some(s)) => s.name,
        _ => req.scenario_id.clone(),
    };

    let plan_exec = PlanExecutor::new(
        Arc::new(CachingSpecSource::new(st.case_specs.clone())),
        Arc::new(NoopSink),
    );
    let executor: Arc<dyn RequestExecutor> =
        Arc::new(ScenarioExecutor { exec: plan_exec, nodes, env });

    let report_id = match st
        .store
        .create(
            &req.project_id,
            "SCENARIO",
            &scenario_name,
            req.concurrency as i32,
            planned_iterations,
        )
        .await
    {
        Ok(id) => id,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "create report error").into_response()
        }
    };

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
        let a = build_assertions(&body(json!({"url": "redis://x", "expectContains": "hello"})));
        assert_eq!(a, vec![ProbeAssertion::OutputContains("hello".into())]);
    }
}
