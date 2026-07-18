use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use api_runner::{Assertion, HttpMethod, MatchCondition, Processor, RequestSpec};
use api_scenario::application::{
    CompileError, CompileScenarioUseCase, RecordScenarioExecutionUseCase,
};
use api_scenario::domain::PlanStep;
use api_test::adapters::local::CaseSpecSource;
use api_test::adapters::plan::{Condition, Leaf, PlanExecutor, PlanNode};
use api_test::adapters::PgBatchReport;
use api_test::ports::EnvironmentPort;
use pool_runner::protocol::{WireEnv, WireNode};
use webauth::{AuthUser, SessionStore};

use crate::pool_runner_ws::{PoolHub, RemoteOutcome, RunCtx};

/// Remote runs that outlive this window are finalized as ERROR.
const REMOTE_RUN_TIMEOUT: Duration = Duration::from_secs(900);

#[derive(Clone)]
struct RunState {
    runner: ScenarioRunner,
    reports: PgBatchReport,
    sessions: Arc<dyn SessionStore>,
}

/// Shared in-process scenario execution: compile → plan nodes → execute → report + execution record.
/// Used by the HTTP run endpoint, the batch-run endpoint, and the cron scheduler.
#[derive(Clone)]
pub struct ScenarioRunner {
    pub compile: CompileScenarioUseCase,
    pub executor: PlanExecutor,
    pub envs: Arc<dyn EnvironmentPort>,
    pub reports: PgBatchReport,
    pub recorder: RecordScenarioExecutionUseCase,
    pub pool: migrate::PgPool,
    /// Case spec source for resolving CASE steps into self-contained wire plans.
    pub specs: Arc<dyn CaseSpecSource>,
    /// Pool-runner hub; None disables remote dispatch and live events (plan runs).
    pub hub: Option<Arc<PoolHub>>,
}

pub enum RunError {
    NotFound,
    CycleOrDepth,
    NoSteps,
    Internal(&'static str),
}

pub struct RunOutcome {
    pub report_id: String,
    pub status: String,
    pub case_count: usize,
}

impl ScenarioRunner {
    /// Compiles a scenario into executable plan nodes (+ static leaf count).
    async fn prepare(&self, scenario_id: &str) -> Result<(Vec<PlanNode>, usize), RunError> {
        let plan = match self.compile.compile_plan(scenario_id).await {
            Ok(p) => p,
            Err(CompileError::NotFound(_)) => return Err(RunError::NotFound),
            Err(CompileError::Cycle(_)) | Err(CompileError::Depth(_)) => {
                return Err(RunError::CycleOrDepth)
            }
            Err(CompileError::Repo(_)) => return Err(RunError::Internal("storage error")),
        };
        let mut once = 0u32;
        let nodes = to_nodes(&plan, &mut once);
        let count = count_leaves(&nodes);
        if count == 0 {
            return Err(RunError::NoSteps);
        }
        Ok((nodes, count))
    }

    async fn resolve_env(
        &self,
        environment_id: Option<&str>,
    ) -> Result<api_test::domain::ResolvedEnv, RunError> {
        match environment_id.filter(|s| !s.trim().is_empty()) {
            Some(eid) => match self.envs.resolve(eid).await {
                Ok(e) => Ok(e.unwrap_or_default()),
                Err(_) => Err(RunError::Internal("env resolve error")),
            },
            None => Ok(Default::default()),
        }
    }

    /// Environment the scenario itself is configured with (meta.envId).
    pub async fn default_env_of(&self, scenario_id: &str) -> Option<String> {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT meta->>'envId' FROM ms_api_scenario WHERE id = $1 AND NOT deleted",
        )
        .bind(scenario_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .filter(|s| !s.trim().is_empty())
    }

    /// Creates the report and returns a completion future without awaiting it,
    /// so the HTTP layer can either await (sync) or spawn it (async + live WS).
    /// Routing: a pool with a live runner gets the compiled plan over WS; no
    /// runner (or unresolvable plan) falls back to in-process execution.
    pub async fn begin(
        &self,
        scenario_id: &str,
        project_id: &str,
        environment_id: Option<&str>,
        stop_on_failure: bool,
        pool_id: Option<&str>,
    ) -> Result<StartedRun, RunError> {
        let (nodes, count) = self.prepare(scenario_id).await?;
        let env = self.resolve_env(environment_id).await?;
        let report_id = self
            .reports
            .create("SERIAL", count as i32)
            .await
            .map_err(|_| RunError::Internal("create report error"))?;
        let step_ids = leaf_ids(&nodes);
        if let Some(hub) = &self.hub {
            hub.events().publish(
                &report_id,
                serde_json::json!({
                    "type": "runStarted", "runId": report_id,
                    "scenarioId": scenario_id, "steps": step_ids,
                }),
            );
        }

        let remote = match (pool_id.filter(|p| !p.trim().is_empty()), &self.hub) {
            (Some(pid), Some(hub)) if hub.has_runner(pid) => {
                match plan_to_wire(&nodes, self.specs.as_ref()).await {
                    Ok(wire) => hub.dispatch(
                        pid,
                        &report_id,
                        wire,
                        WireEnv {
                            base_url: env.base_url.clone(),
                            headers: env.headers.clone(),
                            variables: env.variables.clone(),
                        },
                        stop_on_failure,
                        RunCtx {
                            scenario_id: scenario_id.to_string(),
                            project_id: project_id.to_string(),
                            case_count: count as i32,
                            report_id: report_id.clone(),
                            finalize_report: true,
                        },
                    ),
                    // Unresolvable plan (missing case spec): run locally, which
                    // records the precise per-step error.
                    Err(()) => None,
                }
            }
            _ => None,
        };

        let this = self.clone();
        let sid = scenario_id.to_string();
        let proj = project_id.to_string();
        let rid = report_id.clone();
        let fut: Pin<Box<dyn Future<Output = String> + Send>> = Box::pin(async move {
            if let Some(done) = remote {
                match tokio::time::timeout(REMOTE_RUN_TIMEOUT, done).await {
                    Ok(Ok(RemoteOutcome::Finished(status))) => return status,
                    // Pool lost every runner while the run sat in the queue:
                    // fall through to local execution.
                    Ok(Ok(RemoteOutcome::Fallback)) => {}
                    // Timeout or hub dropped the waiter: finalize as error.
                    _ => {
                        if let Some(hub) = &this.hub {
                            hub.abort_run(&rid).await;
                        }
                        return "ERROR".to_string();
                    }
                }
            }
            let status = match this.executor.run(&rid, &nodes, &env, stop_on_failure).await {
                Ok(true) => "SUCCESS",
                _ => "ERROR",
            };
            let _ = this.reports.set_status(&rid, status).await;
            let _ = this.recorder.execute(&sid, &proj, status, count as i32, Some(&rid)).await;
            if let Some(hub) = &this.hub {
                hub.events().publish(
                    &rid,
                    serde_json::json!({"type": "runComplete", "runId": rid, "status": status}),
                );
            }
            status.to_string()
        });
        Ok(StartedRun { report_id, step_ids, case_count: count, fut })
    }

    /// Executes one member scenario of a union batch into the shared report.
    /// Routed to the pool when it has live runners (unique dispatch run id,
    /// results persisted under the shared report id); otherwise, or on queue
    /// fallback, executed in-process. Records the scenario execution either way.
    #[allow(clippy::too_many_arguments)]
    async fn run_union_member(
        &self,
        report_id: &str,
        scenario_id: &str,
        project_id: &str,
        nodes: &[PlanNode],
        count: usize,
        env: &api_test::domain::ResolvedEnv,
        pool_id: Option<&str>,
    ) -> String {
        if let (Some(pid), Some(hub)) = (pool_id, &self.hub) {
            if hub.has_runner(pid) {
                if let Ok(wire) = plan_to_wire(nodes, self.specs.as_ref()).await {
                    let run_id = uuid::Uuid::new_v4().to_string();
                    let remote = hub.dispatch(
                        pid,
                        &run_id,
                        wire,
                        WireEnv {
                            base_url: env.base_url.clone(),
                            headers: env.headers.clone(),
                            variables: env.variables.clone(),
                        },
                        false,
                        RunCtx {
                            scenario_id: scenario_id.to_string(),
                            project_id: project_id.to_string(),
                            case_count: count as i32,
                            report_id: report_id.to_string(),
                            finalize_report: false,
                        },
                    );
                    if let Some(done) = remote {
                        match tokio::time::timeout(REMOTE_RUN_TIMEOUT, done).await {
                            Ok(Ok(RemoteOutcome::Finished(status))) => return status,
                            Ok(Ok(RemoteOutcome::Fallback)) => {}
                            _ => {
                                hub.abort_run(&run_id).await;
                                return "ERROR".to_string();
                            }
                        }
                    }
                }
            }
        }
        let status = match self.executor.run(report_id, nodes, env, false).await {
            Ok(true) => "SUCCESS",
            _ => "ERROR",
        };
        let _ = self
            .recorder
            .execute(scenario_id, project_id, status, count as i32, Some(report_id))
            .await;
        status.to_string()
    }

    pub async fn run(
        &self,
        scenario_id: &str,
        project_id: &str,
        environment_id: Option<&str>,
        stop_on_failure: bool,
        pool_id: Option<&str>,
    ) -> Result<RunOutcome, RunError> {
        let started =
            self.begin(scenario_id, project_id, environment_id, stop_on_failure, pool_id).await?;
        let status = started.fut.await;
        Ok(RunOutcome { report_id: started.report_id, status, case_count: started.case_count })
    }
}

pub struct StartedRun {
    pub report_id: String,
    pub step_ids: Vec<String>,
    pub case_count: usize,
    /// Resolves to the final report status; report/record writes happen inside.
    pub fut: Pin<Box<dyn Future<Output = String> + Send>>,
}

/// Ordered leaf identities (loop bodies once), matching executor result keys.
fn leaf_ids(nodes: &[PlanNode]) -> Vec<String> {
    let mut out = Vec::new();
    collect_leaf_ids(nodes, &mut out);
    out
}

fn collect_leaf_ids(nodes: &[PlanNode], out: &mut Vec<String>) {
    for n in nodes {
        match n {
            PlanNode::Leaf(Leaf::Case { case_id }) => out.push(case_id.clone()),
            PlanNode::Leaf(Leaf::Request { label, .. }) => out.push(label.clone()),
            PlanNode::Loop { body, .. }
            | PlanNode::If { body, .. }
            | PlanNode::Once { body, .. } => collect_leaf_ids(body, out),
            PlanNode::Timer { .. } => {}
        }
    }
}

/// Executable plan → self-contained wire plan: CASE leaves are resolved to full
/// request specs so the runner needs no database. Err = some spec is missing
/// or unreadable; the caller falls back to local execution.
fn plan_to_wire<'a>(
    nodes: &'a [PlanNode],
    specs: &'a dyn CaseSpecSource,
) -> Pin<Box<dyn Future<Output = Result<Vec<WireNode>, ()>> + Send + 'a>> {
    Box::pin(async move {
        let mut out = Vec::with_capacity(nodes.len());
        for n in nodes {
            out.push(match n {
                PlanNode::Leaf(Leaf::Case { case_id }) => {
                    let spec = specs.spec_of(case_id).await.map_err(|_| ())?.ok_or(())?;
                    WireNode::Step {
                        id: case_id.clone(),
                        request: spec.request,
                        assertions: spec.assertions,
                        processors: spec.processors,
                    }
                }
                PlanNode::Leaf(Leaf::Request { label, request, assertions, processors }) => {
                    WireNode::Step {
                        id: label.clone(),
                        request: request.clone(),
                        assertions: assertions.clone(),
                        processors: processors.clone(),
                    }
                }
                PlanNode::Loop { times, body } => {
                    WireNode::Loop { times: *times, body: plan_to_wire(body, specs).await? }
                }
                PlanNode::If { condition, body } => WireNode::If {
                    variable: condition.variable.clone(),
                    operator: condition.condition,
                    value: condition.value.clone(),
                    body: plan_to_wire(body, specs).await?,
                },
                PlanNode::Once { id, body } => {
                    WireNode::Once { id: *id, body: plan_to_wire(body, specs).await? }
                }
                PlanNode::Timer { ms } => WireNode::Timer { ms: *ms },
            });
        }
        Ok(out)
    })
}

impl FromRef<RunState> for Arc<dyn SessionStore> {
    fn from_ref(s: &RunState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(runner: ScenarioRunner, sessions: Arc<dyn SessionStore>) -> Router {
    let reports = runner.reports.clone();
    Router::new()
        .route("/api/scenario/{id}/run", post(run_scenario))
        .route("/api/scenario/batch-run", post(batch_run_scenarios))
        .route("/api/scenario-report/{report_id}", get(scenario_report))
        .with_state(RunState { runner, reports, sessions })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct BatchRunBody {
    project_id: String,
    scenario_ids: Vec<String>,
    /// Env override; absent = each scenario's own configured environment.
    #[serde(default)]
    environment_id: Option<String>,
    /// SERIAL | PARALLEL (parallel concurrency is bounded by the pool).
    #[serde(default = "default_mode")]
    mode: String,
    /// Serial only: stop the batch at the first failed scenario.
    #[serde(default)]
    stop_on_fail: bool,
    /// true = one combined report for the whole batch (always executes serially).
    #[serde(default)]
    union_report: bool,
    #[serde(default)]
    report_name: Option<String>,
    #[serde(default)]
    pool_id: Option<String>,
}

fn default_mode() -> String {
    "SERIAL".to_string()
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct BatchRunItem {
    scenario_id: String,
    report_id: Option<String>,
    status: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct BatchRunResponse {
    status: String,
    /// Combined report id (union mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    report_id: Option<String>,
    total: usize,
    success: usize,
    results: Vec<BatchRunItem>,
}

/// Resolves the effective env id for one scenario: explicit override wins,
/// otherwise the scenario's own configured environment.
async fn effective_env(
    runner: &ScenarioRunner,
    override_env: Option<&str>,
    scenario_id: &str,
) -> Option<String> {
    match override_env.filter(|s| !s.trim().is_empty()) {
        Some(e) => Some(e.to_string()),
        None => runner.default_env_of(scenario_id).await,
    }
}

#[utoipa::path(post, path = "/api/scenario/batch-run", tag = "api-scenario", request_body = BatchRunBody, responses((status = 200, body = BatchRunResponse), (status = 400), (status = 403)), security(("bearer" = [])))]
async fn batch_run_scenarios(
    user: AuthUser,
    State(st): State<RunState>,
    Json(b): Json<BatchRunBody>,
) -> Response {
    if !user.can("API_SCENARIO", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    if b.scenario_ids.is_empty() {
        return (StatusCode::BAD_REQUEST, "scenarioIds required").into_response();
    }
    // Pool gate: when picked it must exist and be enabled; its max_concurrency
    // bounds parallel execution.
    let mut concurrency = 4usize;
    let pool_id = b.pool_id.as_deref().filter(|s| !s.trim().is_empty());
    if let Some(pid) = pool_id {
        let row = sqlx::query(
            "SELECT enabled, max_concurrency FROM ms_resource_pool WHERE id = $1 AND NOT deleted",
        )
        .bind(pid)
        .fetch_optional(&st.runner.pool)
        .await;
        match row {
            Ok(Some(r)) => {
                use sqlx::Row;
                if !r.try_get::<bool, _>("enabled").unwrap_or(false) {
                    return (StatusCode::BAD_REQUEST, "resource pool disabled").into_response();
                }
                concurrency = r.try_get::<i32, _>("max_concurrency").unwrap_or(4).max(1) as usize;
            }
            Ok(None) => {
                return (StatusCode::BAD_REQUEST, "resource pool not found").into_response()
            }
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
        }
    }
    let override_env = b.environment_id.as_deref();

    if b.union_report {
        // Combined report: compile everything first, then execute serially into
        // one report so step ordering follows the selection.
        let mut prepared = Vec::new();
        let mut results = Vec::new();
        let mut total_count = 0usize;
        for sid in &b.scenario_ids {
            match st.runner.prepare(sid).await {
                Ok((nodes, count)) => {
                    total_count += count;
                    prepared.push((sid.clone(), nodes, count));
                }
                Err(_) => results.push(BatchRunItem {
                    scenario_id: sid.clone(),
                    report_id: None,
                    status: "ERROR".to_string(),
                }),
            }
        }
        if prepared.is_empty() {
            return (StatusCode::BAD_REQUEST, "no runnable scenarios").into_response();
        }
        let report_id = match st.reports.create("SERIAL", total_count as i32).await {
            Ok(r) => r,
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "create report error").into_response()
            }
        };
        let name = b.report_name.as_deref().unwrap_or("").trim();
        let _ = sqlx::query("UPDATE ms_api_batch_report SET name = $2, pool_id = $3 WHERE id = $1")
            .bind(&report_id)
            .bind(name)
            .bind(pool_id.unwrap_or(""))
            .execute(&st.runner.pool)
            .await;
        let mut success = 0usize;
        let prepared_len = prepared.len();
        let pool_live = pool_id
            .map(|p| st.runner.hub.as_ref().is_some_and(|h| h.has_runner(p)))
            .unwrap_or(false);
        if pool_live && !b.stop_on_fail && prepared_len > 1 {
            // Members run in parallel through the pool, bounded by the batch
            // concurrency; runner caps + server-side queueing serialize the
            // overflow. All results land in the one shared report.
            let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
            let mut handles = Vec::new();
            for (sid, nodes, count) in prepared {
                let runner = st.runner.clone();
                let sem = sem.clone();
                let project = b.project_id.clone();
                let override_env = override_env.map(str::to_string);
                let report = report_id.clone();
                let pid = pool_id.map(str::to_string);
                let sid2 = sid.clone();
                handles.push((
                    sid,
                    tokio::spawn(async move {
                        let _permit = sem.acquire().await;
                        let env_id = effective_env(&runner, override_env.as_deref(), &sid2).await;
                        let env = runner.resolve_env(env_id.as_deref()).await.unwrap_or_default();
                        runner
                            .run_union_member(
                                &report,
                                &sid2,
                                &project,
                                &nodes,
                                count,
                                &env,
                                pid.as_deref(),
                            )
                            .await
                    }),
                ));
            }
            for (sid, h) in handles {
                let status = h.await.unwrap_or_else(|_| "ERROR".to_string());
                if status == "SUCCESS" {
                    success += 1;
                }
                results.push(BatchRunItem {
                    scenario_id: sid,
                    report_id: Some(report_id.clone()),
                    status,
                });
            }
        } else {
            // Serial members (stop_on_fail, single member, or no live runner);
            // each still routes through the pool when one is available.
            let mut stopped = false;
            for (sid, nodes, count) in &prepared {
                if stopped {
                    results.push(BatchRunItem {
                        scenario_id: sid.clone(),
                        report_id: Some(report_id.clone()),
                        status: "SKIPPED".to_string(),
                    });
                    continue;
                }
                let env_id = effective_env(&st.runner, override_env, sid).await;
                let env = st.runner.resolve_env(env_id.as_deref()).await.unwrap_or_default();
                let status = st
                    .runner
                    .run_union_member(&report_id, sid, &b.project_id, nodes, *count, &env, pool_id)
                    .await;
                if status == "SUCCESS" {
                    success += 1;
                } else if b.stop_on_fail {
                    stopped = true;
                }
                results.push(BatchRunItem {
                    scenario_id: sid.clone(),
                    report_id: Some(report_id.clone()),
                    status,
                });
            }
        }
        let overall = if success == prepared_len { "SUCCESS" } else { "ERROR" };
        let _ = st.reports.set_status(&report_id, overall).await;
        return Json(BatchRunResponse {
            status: overall.to_string(),
            report_id: Some(report_id),
            total: b.scenario_ids.len(),
            success,
            results,
        })
        .into_response();
    }

    // Independent reports: serial honors stop_on_fail; parallel is bounded by
    // the pool's max_concurrency via a semaphore.
    let mut results = Vec::new();
    let mut success = 0usize;
    if b.mode.eq_ignore_ascii_case("PARALLEL") {
        let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut handles = Vec::new();
        for sid in &b.scenario_ids {
            let runner = st.runner.clone();
            let sem = sem.clone();
            let sid = sid.clone();
            let project = b.project_id.clone();
            let override_env = override_env.map(|s| s.to_string());
            let pool = pool_id.map(str::to_string);
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let env_id = effective_env(&runner, override_env.as_deref(), &sid).await;
                let out =
                    runner.run(&sid, &project, env_id.as_deref(), false, pool.as_deref()).await;
                (sid, out)
            }));
        }
        for h in handles {
            match h.await {
                Ok((sid, Ok(o))) => {
                    if o.status == "SUCCESS" {
                        success += 1;
                    }
                    results.push(BatchRunItem {
                        scenario_id: sid,
                        report_id: Some(o.report_id),
                        status: o.status,
                    });
                }
                Ok((sid, Err(_))) => results.push(BatchRunItem {
                    scenario_id: sid,
                    report_id: None,
                    status: "ERROR".to_string(),
                }),
                Err(_) => {}
            }
        }
    } else {
        let mut stopped = false;
        for sid in &b.scenario_ids {
            if stopped {
                results.push(BatchRunItem {
                    scenario_id: sid.clone(),
                    report_id: None,
                    status: "SKIPPED".to_string(),
                });
                continue;
            }
            let env_id = effective_env(&st.runner, override_env, sid).await;
            match st.runner.run(sid, &b.project_id, env_id.as_deref(), false, pool_id).await {
                Ok(o) => {
                    if o.status == "SUCCESS" {
                        success += 1;
                    } else if b.stop_on_fail {
                        stopped = true;
                    }
                    results.push(BatchRunItem {
                        scenario_id: sid.clone(),
                        report_id: Some(o.report_id),
                        status: o.status,
                    });
                }
                Err(_) => {
                    if b.stop_on_fail {
                        stopped = true;
                    }
                    results.push(BatchRunItem {
                        scenario_id: sid.clone(),
                        report_id: None,
                        status: "ERROR".to_string(),
                    });
                }
            }
        }
    }
    let overall = if success == b.scenario_ids.len() { "SUCCESS" } else { "ERROR" };
    Json(BatchRunResponse {
        status: overall.to_string(),
        report_id: None,
        total: b.scenario_ids.len(),
        success,
        results,
    })
    .into_response()
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunScenarioBody {
    project_id: String,
    #[serde(default)]
    environment_id: Option<String>,
    #[serde(default = "default_strategy")]
    failure_strategy: String,
    /// Resource pool routing: a pool with a connected runner executes remotely.
    #[serde(default)]
    pool_id: Option<String>,
    /// true = return immediately with status RUNNING; progress streams over
    /// /api/run-events/ws?runId={reportId}.
    #[serde(default)]
    async_run: bool,
}

fn default_strategy() -> String {
    "CONTINUE".to_string()
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ScenarioReportResponse {
    report_id: String,
    status: String,
    case_count: i32,
    /// Display name (union batch reports); empty for single runs.
    #[serde(skip_serializing_if = "String::is_empty")]
    name: String,
    started_at: Option<String>,
    finished_at: Option<String>,
    duration_ms: Option<i64>,
    results: Vec<ReportResultItem>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReportResultItem {
    case_id: String,
    outcome: String,
    failures: Vec<String>,
    executed_at: String,
    status_code: Option<i32>,
    latency_ms: Option<i64>,
    resp_size: Option<i64>,
    body: Option<String>,
    headers: Vec<(String, String)>,
    #[schema(value_type = Vec<Object>)]
    assertions: serde_json::Value,
    #[schema(value_type = Vec<Object>)]
    extractions: serde_json::Value,
    /// Per-phase HTTP timings (dnsMs / ttfbMs / downloadMs) when captured.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Object)]
    timings: Option<serde_json::Value>,
    request: Option<ReqInfo>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReqInfo {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

#[utoipa::path(get, path = "/api/scenario-report/{report_id}", tag = "api-scenario", params(("report_id" = String, Path)), responses((status = 200, body = ScenarioReportResponse), (status = 404)), security(("bearer" = [])))]
async fn scenario_report(
    _user: AuthUser,
    State(st): State<RunState>,
    Path(report_id): Path<String>,
) -> Response {
    let name = sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(name, '') FROM ms_api_batch_report WHERE id = $1",
    )
    .bind(&report_id)
    .fetch_optional(&st.runner.pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_default();
    match st.reports.detail(&report_id).await {
        Ok(Some(d)) => (
            StatusCode::OK,
            Json(ScenarioReportResponse {
                report_id,
                status: d.status,
                case_count: d.case_count,
                name,
                started_at: d.started_at,
                finished_at: d.finished_at,
                duration_ms: d.duration_ms,
                results: d
                    .results
                    .into_iter()
                    .map(|r| ReportResultItem {
                        case_id: r.case_id,
                        outcome: r.outcome,
                        failures: r.failures,
                        executed_at: r.executed_at,
                        status_code: r.status_code,
                        latency_ms: r.latency_ms,
                        resp_size: r.resp_size,
                        body: r.body,
                        headers: r.headers,
                        assertions: r.assertions,
                        extractions: r.extractions,
                        timings: r.timings.clone(),
                        request: r.req_method.clone().map(|method| ReqInfo {
                            method,
                            url: r.req_url.clone().unwrap_or_default(),
                            headers: r.req_headers.clone(),
                            body: r.req_body.clone(),
                        }),
                    })
                    .collect(),
            }),
        )
            .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "report not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunScenarioResponse {
    report_id: String,
    /// SUCCESS | ERROR, or RUNNING for asyncRun.
    status: String,
    case_count: usize,
    /// Ordered step identities for live progress rendering.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    steps: Vec<String>,
}

fn op_to_condition(op: &str) -> MatchCondition {
    serde_json::from_value(serde_json::Value::String(op.to_uppercase()))
        .unwrap_or(MatchCondition::Equals)
}

fn method_of(m: &str) -> HttpMethod {
    serde_json::from_value(serde_json::Value::String(m.to_uppercase())).unwrap_or(HttpMethod::Get)
}

fn parse_kv(v: &serde_json::Value) -> Vec<(String, String)> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|it| {
                    if it.get("enabled").and_then(|x| x.as_bool()) == Some(false) {
                        return None;
                    }
                    let k = it.get("key")?.as_str()?.trim();
                    if k.is_empty() {
                        return None;
                    }
                    Some((
                        k.to_string(),
                        it.get("value").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn auth_header(v: &serde_json::Value) -> Option<(String, String)> {
    let token = v.get("token").and_then(|x| x.as_str()).unwrap_or("").trim();
    if token.is_empty() {
        return None;
    }
    match v.get("type").and_then(|x| x.as_str()).unwrap_or("none") {
        "bearer" => Some(("Authorization".to_string(), format!("Bearer {token}"))),
        "basic" => Some(("Authorization".to_string(), format!("Basic {token}"))),
        _ => None,
    }
}

fn merge_query(url: &str, query: &[(String, String)]) -> String {
    if query.is_empty() {
        return url.to_string();
    }
    let qs = query.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&");
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}{qs}")
}

pub(crate) fn to_nodes(steps: &[PlanStep], once: &mut u32) -> Vec<PlanNode> {
    steps.iter().map(|s| to_node(s, once)).collect()
}

fn to_node(step: &PlanStep, once: &mut u32) -> PlanNode {
    match step {
        PlanStep::Case(id) => PlanNode::Leaf(Leaf::Case { case_id: id.clone() }),
        PlanStep::Request(r) => {
            let mut headers = parse_kv(&r.headers);
            if let Some(a) = auth_header(&r.auth) {
                headers.push(a);
            }
            let mut url = r.url.clone();
            for (k, v) in parse_kv(&r.rest_params) {
                url = url.replace(&format!("{{{k}}}"), &v);
            }
            url = merge_query(&url, &parse_kv(&r.query_params));
            PlanNode::Leaf(Leaf::Request {
                label: format!("{} {}", r.method, url),
                request: RequestSpec {
                    method: method_of(&r.method),
                    url,
                    headers,
                    body: r.body.clone(),
                },
                assertions: serde_json::from_value::<Vec<Assertion>>(r.assertions.clone())
                    .unwrap_or_default(),
                processors: serde_json::from_value::<Vec<Processor>>(r.processors.clone())
                    .unwrap_or_default(),
            })
        }
        PlanStep::Loop { times, body } => {
            PlanNode::Loop { times: *times, body: to_nodes(body, once) }
        }
        PlanStep::If { variable, operator, value, body } => PlanNode::If {
            condition: Condition {
                variable: variable.clone(),
                condition: op_to_condition(operator),
                value: value.clone(),
            },
            body: to_nodes(body, once),
        },
        PlanStep::Once { body } => {
            *once += 1;
            PlanNode::Once { id: *once, body: to_nodes(body, once) }
        }
        PlanStep::Timer { ms } => PlanNode::Timer { ms: *ms },
    }
}

/// Static leaf count: a loop body counts once (not multiplied by `times`).
pub(crate) fn count_leaves(nodes: &[PlanNode]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            PlanNode::Leaf(_) => 1,
            PlanNode::Loop { body, .. }
            | PlanNode::If { body, .. }
            | PlanNode::Once { body, .. } => count_leaves(body),
            PlanNode::Timer { .. } => 0,
        })
        .sum()
}

#[utoipa::path(
    post,
    path = "/api/scenario/{id}/run",
    tag = "api-scenario",
    params(("id" = String, Path)),
    request_body = RunScenarioBody,
    responses(
        (status = 200, body = RunScenarioResponse),
        (status = 400),
        (status = 404),
        (status = 409)
    ),
    security(("bearer" = []))
)]
async fn run_scenario(
    user: AuthUser,
    State(st): State<RunState>,
    Path(id): Path<String>,
    Json(req): Json<RunScenarioBody>,
) -> Response {
    if !user.can("API_SCENARIO", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let stop_on_failure = req.failure_strategy.eq_ignore_ascii_case("STOP");
    if req.async_run {
        // Live mode: report id now, execution in the background, events over WS.
        return match st
            .runner
            .begin(
                &id,
                &req.project_id,
                req.environment_id.as_deref(),
                stop_on_failure,
                req.pool_id.as_deref(),
            )
            .await
        {
            Ok(started) => {
                let report_id = started.report_id.clone();
                let steps = started.step_ids.clone();
                let case_count = started.case_count;
                tokio::spawn(started.fut);
                (
                    StatusCode::OK,
                    Json(RunScenarioResponse {
                        report_id,
                        status: "RUNNING".to_string(),
                        case_count,
                        steps,
                    }),
                )
                    .into_response()
            }
            Err(e) => run_error_response(e),
        };
    }
    match st
        .runner
        .run(
            &id,
            &req.project_id,
            req.environment_id.as_deref(),
            stop_on_failure,
            req.pool_id.as_deref(),
        )
        .await
    {
        Ok(o) => (
            StatusCode::OK,
            Json(RunScenarioResponse {
                report_id: o.report_id,
                status: o.status,
                case_count: o.case_count,
                steps: Vec::new(),
            }),
        )
            .into_response(),
        Err(e) => run_error_response(e),
    }
}

fn run_error_response(e: RunError) -> Response {
    match e {
        RunError::NotFound => (StatusCode::NOT_FOUND, "scenario not found").into_response(),
        RunError::CycleOrDepth => {
            (StatusCode::CONFLICT, "scenario reference cycle or too deep").into_response()
        }
        RunError::NoSteps => {
            (StatusCode::BAD_REQUEST, "scenario has no runnable steps").into_response()
        }
        RunError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(run_scenario, batch_run_scenarios),
    components(schemas(RunScenarioBody, RunScenarioResponse, BatchRunBody, BatchRunResponse, BatchRunItem)),
    tags((name = "api-scenario", description = "场景"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_scenario::domain::InlineRequest;

    #[test]
    fn inline_request_assertions_flow_into_leaf() {
        let req = InlineRequest::new("GET", "http://x/ok", None).expect("valid").with_assertions(
            serde_json::json!([
                {"type": "StatusIs", "args": 200},
                {"type": "BodyContains", "args": "ok"}
            ]),
        );
        let mut once = 0;
        match to_node(&PlanStep::Request(req), &mut once) {
            PlanNode::Leaf(Leaf::Request { assertions, .. }) => {
                assert_eq!(
                    assertions,
                    vec![Assertion::StatusIs(200), Assertion::BodyContains("ok".into())]
                );
            }
            other => panic!("expected Leaf::Request, got {other:?}"),
        }
    }

    #[test]
    fn inline_request_without_assertions_is_empty() {
        let req = InlineRequest::new("GET", "http://x/ok", None).expect("valid");
        let mut once = 0;
        match to_node(&PlanStep::Request(req), &mut once) {
            PlanNode::Leaf(Leaf::Request { assertions, .. }) => assert!(assertions.is_empty()),
            other => panic!("expected Leaf::Request, got {other:?}"),
        }
    }
}
