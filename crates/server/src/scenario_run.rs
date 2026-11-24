use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::environment_extension::ExecutionResolver;
use api_runner::{Assertion, HttpMethod, MatchCondition, RequestSpec};
use api_scenario::application::{
    CompileError, CompileScenarioUseCase, RecordScenarioExecutionUseCase,
};
use api_scenario::domain::PlanStep;
use api_test::adapters::local::CaseSpecSource;
use api_test::adapters::plan::{Condition, Leaf, PlanExecutor, PlanNode, StepPreparation};
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
    pub preparation: Option<ExecutionResolver>,
    pub actor: Option<AuthUser>,
    pub envs: Arc<dyn EnvironmentPort>,
    pub reports: PgBatchReport,
    pub recorder: RecordScenarioExecutionUseCase,
    pub pool: migrate::PgPool,
    /// Case spec source for resolving CASE steps into self-contained wire plans.
    pub specs: Arc<dyn CaseSpecSource>,
    /// Pool-runner hub; None disables remote dispatch and live events (plan runs).
    pub hub: Option<Arc<PoolHub>>,
    /// Failure notifications to the scenario creator; None for embedded runs
    /// (plan entries roll failures up into the plan result instead).
    pub notifier: Option<notice::application::Notifier>,
}

struct ScenarioPreparation {
    resolver: ExecutionResolver,
    pool: migrate::PgPool,
    scenario_id: Option<String>,
    actor: AuthUser,
    project_id: String,
    environment_id: Option<String>,
}

#[async_trait::async_trait]
impl StepPreparation for ScenarioPreparation {
    async fn prepare(
        &self,
        variables: std::collections::BTreeMap<String, String>,
        processors: &[api_runner::Processor],
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        self.prepare_scoped(variables, processors, false).await
    }

    async fn plugin_step(
        &self,
        variables: std::collections::BTreeMap<String, String>,
        processor: &api_runner::Processor,
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        self.prepare_scoped(variables, std::slice::from_ref(processor), true).await
    }
}

impl ScenarioPreparation {
    async fn prepare_scoped(
        &self,
        variables: std::collections::BTreeMap<String, String>,
        processors: &[api_runner::Processor],
        plugin_step: bool,
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        if let Some(scenario_id) = &self.scenario_id {
            let belongs_to_project: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM ms_api_scenario WHERE id = $1 AND project_id = $2 AND NOT deleted)",
        )
        .bind(scenario_id)
        .bind(&self.project_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| "could not resolve scenario project".to_owned())?;
            if !belongs_to_project {
                return Err("scenario does not belong to the execution project".into());
            }
        }
        let prepared = if plugin_step
            && !matches!(processors, [api_runner::Processor::SandboxScript { .. }])
        {
            let environment =
                self.environment_id.as_deref().ok_or("select an execution environment")?;
            self.resolver
                .prepare_scenario_step(
                    self.actor.clone(),
                    &self.project_id,
                    environment,
                    variables,
                    processors,
                )
                .await
        } else {
            self.resolver
                .prepare_request_project(
                    self.actor.clone(),
                    &self.project_id,
                    self.environment_id.as_deref(),
                    variables,
                    processors,
                )
                .await
        };
        prepared.map(|prepared| prepared.variables).map_err(|response| {
            format!("scenario plugin execution failed (HTTP {})", response.status().as_u16())
        })
    }
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
    /// Where the run executed remotely; None = in-process (local).
    pub executed_on: Option<ExecutedOn>,
}

/// Remote execution location surfaced in run responses (null = local).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutedOn {
    pub pool_id: String,
    pub pool_name: String,
    /// Assigned runner's display name; empty while the run sat queued.
    pub runner: String,
}

/// Shared slot: begin() records the dispatch target, the run future clears it
/// when the run falls back to local execution.
pub type ExecutedOnSlot = Arc<Mutex<Option<ExecutedOn>>>;

impl ScenarioRunner {
    /// Compiles a scenario into executable plan nodes (+ static leaf count).
    fn executor_for(
        &self,
        scenario_id: &str,
        project_id: &str,
        environment_id: Option<&str>,
    ) -> PlanExecutor {
        let executor = self.executor.clone();
        match (&self.preparation, &self.actor, environment_id) {
            (Some(resolver), Some(actor), environment_id) => {
                executor.with_preparation(Arc::new(ScenarioPreparation {
                    resolver: resolver.clone(),
                    pool: self.pool.clone(),
                    scenario_id: Some(scenario_id.to_owned()),
                    actor: actor.clone(),
                    project_id: project_id.to_owned(),
                    environment_id: environment_id.map(str::to_owned),
                }))
            }
            _ => executor,
        }
    }

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

    /// Capability tags the scenario requires of a runner (meta.capabilities,
    /// a JSON string array). Empty = any runner. No UI writes this field yet;
    /// it is the designated requirement source for capability matching.
    async fn required_caps_of(&self, scenario_id: &str) -> Vec<String> {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT meta->>'capabilities' FROM ms_api_scenario WHERE id = $1 AND NOT deleted",
        )
        .bind(scenario_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
    }

    /// Display name of a live pool (empty when unknown).
    async fn pool_name_of(&self, pool_id: &str) -> String {
        sqlx::query_scalar::<_, String>(
            "SELECT name FROM ms_resource_pool WHERE id = $1 AND NOT deleted",
        )
        .bind(pool_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
    }

    /// Picks a pool for a run without an explicit poolId: enabled live pools
    /// applicable to the project's organization (all-org or listed) that have
    /// online runners, preferring the most spare capacity. None = run locally.
    pub async fn auto_pool(&self, project_id: &str) -> Option<(String, String)> {
        let hub = self.hub.as_ref()?;
        let detail = hub.online_detail();
        if detail.is_empty() {
            return None;
        }
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT p.id, p.name FROM ms_resource_pool p \
             WHERE p.enabled AND NOT p.deleted \
               AND (p.all_org OR p.org_ids ? \
                    (SELECT organization_id FROM ms_project WHERE id = $1))",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .ok()?;
        let spare = |id: &str| {
            detail
                .get(id)
                .map(|rs| {
                    rs.iter().map(|r| r.max_concurrent.saturating_sub(r.in_flight)).sum::<usize>()
                })
                .unwrap_or(0)
        };
        let (id, name) = rows
            .into_iter()
            .filter(|(id, _)| detail.contains_key(id))
            .max_by_key(|(id, _)| spare(id))?;
        tracing::info!(pool = %id, %name, project = %project_id, "auto-selected resource pool for run");
        Some((id, name))
    }

    /// Creates the report and returns a completion future without awaiting it,
    /// so the HTTP layer can either await (sync) or spawn it (async + live WS).
    /// Routing: an explicit pool wins; without one an applicable pool with
    /// online runners is auto-picked. A pool with a live runner gets the
    /// compiled plan over WS; no runner (or unresolvable plan) falls back to
    /// in-process execution.
    pub async fn begin(
        &self,
        scenario_id: &str,
        project_id: &str,
        environment_id: Option<&str>,
        stop_on_failure: bool,
        pool_id: Option<&str>,
        triggered_by: &str,
    ) -> Result<StartedRun, RunError> {
        let (nodes, count) = self.prepare(scenario_id).await?;
        // Env fallback: when the caller does not pin an environment, use the
        // scenario's own configured environment (meta.envId) so scenarios that
        // declare a default base URL still resolve relative paths correctly.
        let effective_env = match environment_id.filter(|s| !s.trim().is_empty()) {
            Some(eid) => Some(eid.to_string()),
            None => self.default_env_of(scenario_id).await,
        };
        let env = self.resolve_env(effective_env.as_deref()).await?;
        let report_id = self
            .reports
            .create("SERIAL", count as i32)
            .await
            .map_err(|_| RunError::Internal("create report error"))?;
        let step_ids = leaf_ids(&nodes);
        if !triggered_by.is_empty() {
            if let Err(error) = sqlx::query(
                "UPDATE ms_api_batch_report SET scenario_triggered_by = $2 WHERE id = $1",
            )
            .bind(&report_id)
            .bind(triggered_by)
            .execute(&self.pool)
            .await
            {
                tracing::warn!(%report_id, %error, "Could not record scenario trigger");
            }
        }
        if let Some(hub) = &self.hub {
            hub.events().publish(
                &report_id,
                serde_json::json!({
                    "type": "runStarted", "runId": report_id,
                    "scenarioId": scenario_id, "steps": step_ids,
                }),
            );
        }

        // Effective pool: explicit request wins; otherwise auto-pick.
        let picked = match pool_id.filter(|p| !p.trim().is_empty()) {
            Some(pid) => Some((pid.to_string(), self.pool_name_of(pid).await)),
            None => self.auto_pool(project_id).await,
        };
        let executed_on: ExecutedOnSlot = Arc::new(Mutex::new(None));
        let required = self.required_caps_of(scenario_id).await;
        let remote = match (&picked, &self.hub) {
            (Some((pid, pname)), Some(hub)) if hub.has_runner(pid) => {
                match plan_to_wire(&nodes, self.specs.as_ref()).await {
                    Ok(wire) => hub
                        .dispatch(
                            pid,
                            &report_id,
                            required,
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
                        )
                        .map(|(done, runner)| {
                            *executed_on
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                Some(ExecutedOn {
                                    pool_id: pid.clone(),
                                    pool_name: pname.clone(),
                                    runner: runner.unwrap_or_default(),
                                });
                            done
                        }),
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
        let executed_slot = executed_on.clone();
        let fut: Pin<Box<dyn Future<Output = String> + Send>> = Box::pin(async move {
            if let Some(done) = remote {
                match tokio::time::timeout(REMOTE_RUN_TIMEOUT, done).await {
                    Ok(Ok(RemoteOutcome::Finished(status))) => return status,
                    // Pool lost every runner while the run sat in the queue:
                    // fall through to local execution.
                    Ok(Ok(RemoteOutcome::Fallback)) => {
                        *executed_slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                            None;
                    }
                    // Timeout or hub dropped the waiter: finalize as error.
                    _ => {
                        if let Some(hub) = &this.hub {
                            hub.abort_run(&rid).await;
                        }
                        return "ERROR".to_string();
                    }
                }
            }
            let status = match this
                .executor_for(&sid, &proj, effective_env.as_deref())
                .run(&rid, &nodes, &env, stop_on_failure)
                .await
            {
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
        let fut: Pin<Box<dyn Future<Output = String> + Send>> = match self.notifier.clone() {
            Some(notifier) => {
                let db = self.pool.clone();
                let sid = scenario_id.to_string();
                let proj = project_id.to_string();
                let rid = report_id.clone();
                let trigger = triggered_by.to_owned();
                Box::pin(async move {
                    let status = fut.await;
                    notify_scenario_result(
                        &db,
                        &notifier,
                        ScenarioNoticeTarget::Single(&sid),
                        &proj,
                        &rid,
                        &status,
                        &trigger,
                    )
                    .await;
                    status
                })
            }
            None => fut,
        };
        Ok(StartedRun { report_id, step_ids, case_count: count, executed_on, fut })
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
        environment_id: Option<&str>,
    ) -> String {
        if let (Some(pid), Some(hub)) = (pool_id, &self.hub) {
            if hub.has_runner(pid) {
                if let Ok(wire) = plan_to_wire(nodes, self.specs.as_ref()).await {
                    let run_id = uuid::Uuid::new_v4().to_string();
                    let required = self.required_caps_of(scenario_id).await;
                    let remote = hub.dispatch(
                        pid,
                        &run_id,
                        required,
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
                    if let Some((done, _runner)) = remote {
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
        let status = match self
            .executor_for(scenario_id, project_id, environment_id)
            .run(report_id, nodes, env, false)
            .await
        {
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
        triggered_by: &str,
    ) -> Result<RunOutcome, RunError> {
        let started = self
            .begin(scenario_id, project_id, environment_id, stop_on_failure, pool_id, triggered_by)
            .await?;
        let status = started.fut.await;
        // Read after completion: the future clears the slot on local fallback.
        let executed_on =
            started.executed_on.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        Ok(RunOutcome {
            report_id: started.report_id,
            status,
            case_count: started.case_count,
            executed_on,
        })
    }
}

enum ScenarioNoticeTarget<'a> {
    Single(&'a str),
    Batch(&'a str),
}

async fn notify_scenario_result(
    pool: &migrate::PgPool,
    notifier: &notice::application::Notifier,
    target: ScenarioNoticeTarget<'_>,
    project_id: &str,
    report_id: &str,
    status: &str,
    triggered_by: &str,
) {
    use crate::scenario_notice::{scenario_notice_content, NoticeCase};
    use sqlx::Row as _;
    let (name, receiver, resource_type, resource_id) = match target {
        ScenarioNoticeTarget::Single(scenario_id) => {
            let row = sqlx::query_as::<_, (String, Option<String>)>(
                "SELECT name, created_by FROM ms_api_scenario WHERE id = $1 AND project_id = $2",
            )
            .bind(scenario_id)
            .bind(project_id)
            .fetch_optional(pool)
            .await;
            let Ok(Some((name, creator))) = row else { return };
            (name, creator.unwrap_or_else(|| triggered_by.to_owned()), "SCENARIO", scenario_id)
        }
        ScenarioNoticeTarget::Batch(name) => {
            let title = if name.is_empty() { "批量场景执行" } else { name };
            (title.to_owned(), triggered_by.to_owned(), "SCENARIO_BATCH", report_id)
        }
    };
    let rows = sqlx::query(
        "SELECT COALESCE(c.name, r.case_id) AS name, r.outcome, r.assertions, r.status_code
         FROM ms_api_case_result r LEFT JOIN ms_api_case c ON c.id = r.case_id
         WHERE r.report_id = $1 ORDER BY r.executed_at, r.case_id",
    )
    .bind(report_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let cases: Vec<_> = rows
        .iter()
        .map(|row| NoticeCase {
            name: row.try_get("name").unwrap_or_default(),
            outcome: row.try_get("outcome").unwrap_or_default(),
            assertions: row.try_get("assertions").unwrap_or_default(),
            status_code: row.try_get("status_code").ok().flatten(),
        })
        .collect();
    let report_url = notice::adapters::report_address::effective(pool).await.map(|base| {
        format!("{}/share/report/{report_id}?access=project", base.trim_end_matches('/'))
    });
    let content = scenario_notice_content(status, &cases, report_url.as_deref());
    notifier
        .notify(
            vec![receiver],
            notice::application::NoticeEvent {
                project_id: project_id.to_string(),
                category: "API".into(),
                event_type: if status == "SUCCESS" {
                    "SCENARIO_RUN_SUCCEEDED"
                } else {
                    "SCENARIO_RUN_FAILED"
                }
                .into(),
                title: name,
                content,
                resource_type: resource_type.into(),
                resource_id: resource_id.to_owned(),
                operator: triggered_by.to_owned(),
            },
        )
        .await;
}

pub struct StartedRun {
    pub report_id: String,
    pub step_ids: Vec<String>,
    pub case_count: usize,
    /// Dispatch target; cleared by the future when execution falls back local.
    pub executed_on: ExecutedOnSlot,
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
            PlanNode::Leaf(Leaf::Request { label, .. }) | PlanNode::Plugin { label, .. } => {
                out.push(label.clone())
            }
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
                    if api_runner::domain::preparation::needs_plugin_preparation(&spec.processors) {
                        return Err(());
                    }
                    WireNode::Step {
                        id: case_id.clone(),
                        request: spec.request,
                        assertions: spec.assertions,
                        processors: spec.processors,
                    }
                }
                PlanNode::Leaf(Leaf::Request { label, request, assertions, processors }) => {
                    if api_runner::domain::preparation::needs_plugin_preparation(processors) {
                        return Err(());
                    }
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
                PlanNode::Plugin { .. } => return Err(()),
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
        .route("/api/case/{id}/run-scripted", post(run_scripted_case))
        .route("/api/scenario/batch-run", post(batch_run_scenarios))
        .route("/api/scenario/regression-activity", get(regression_activity))
        .route("/api/scenario-report/{report_id}", get(scenario_report))
        .route("/api/scenario-report/{report_id}/share", post(create_scenario_share))
        // Public: no auth — resolves a share token to the report. Mounted here so it shares state.
        .route("/public/scenario-report/{token}", get(public_scenario_report))
        .with_state(RunState { runner, reports, sessions })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScriptedCaseBody {
    project_id: String,
    environment_id: Option<String>,
}

async fn run_scripted_case(
    user: AuthUser,
    State(state): State<RunState>,
    Path(case_id): Path<String>,
    Json(body): Json<ScriptedCaseBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !user.can("API_DEFINITION", "EXECUTE") {
        return Err(StatusCode::FORBIDDEN);
    }
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM ms_api_case WHERE id=$1 AND project_id=$2)",
    )
    .bind(&case_id)
    .bind(&body.project_id)
    .fetch_one(&state.runner.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !belongs {
        return Err(StatusCode::NOT_FOUND);
    }
    let spec = state
        .runner
        .specs
        .spec_of(&case_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if !spec.processors.iter().any(|p| matches!(p, api_runner::Processor::SandboxScript { .. })) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let environment = body.environment_id.as_deref();
    let env = state.runner.resolve_env(environment).await.map_err(|_| StatusCode::BAD_REQUEST)?;
    let Some(resolver) = state.runner.preparation.clone() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let report =
        state.reports.create("SERIAL", 1).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let executor = state.runner.executor.clone().with_preparation(Arc::new(ScenarioPreparation {
        resolver,
        pool: state.runner.pool.clone(),
        scenario_id: None,
        actor: user,
        project_id: body.project_id,
        environment_id: body.environment_id,
    }));
    let nodes = [PlanNode::Leaf(Leaf::Case { case_id })];
    let status = if executor.run(&report, &nodes, &env, true).await.unwrap_or(false) {
        "SUCCESS"
    } else {
        "ERROR"
    };
    state
        .reports
        .set_status(&report, status)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "reportId": report, "status": status })))
}

#[derive(Deserialize)]
struct RegressionActivityQuery {
    days: u16,
}

#[derive(Serialize, sqlx::FromRow)]
struct RegressionActivityDay {
    member: String,
    date: String,
    passed: i64,
    evaluated: i64,
}

async fn regression_activity(
    user: AuthUser,
    State(state): State<RunState>,
    Query(query): Query<RegressionActivityQuery>,
) -> Result<Json<Vec<RegressionActivityDay>>, StatusCode> {
    if !user.can("PROJECT", "READ") || !user.can("API_SCENARIO", "READ") {
        return Err(StatusCode::FORBIDDEN);
    }
    if !(1..=365).contains(&query.days) {
        return Err(StatusCode::BAD_REQUEST);
    }
    sqlx::query_as(include_str!("regression_activity.sql"))
        .bind(&user.user_id)
        .bind(i32::from(query.days))
        .bind(user.can("SYSTEM_USER", "UPDATE"))
        .fetch_all(&state.runner.pool)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(%error, "Could not read regression activity");
            StatusCode::INTERNAL_SERVER_ERROR
        })
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
    State(mut st): State<RunState>,
    Json(b): Json<BatchRunBody>,
) -> Response {
    if !user.can("API_SCENARIO", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    st.runner.actor = Some(user.clone());
    if b.scenario_ids.is_empty() {
        return (StatusCode::BAD_REQUEST, "scenarioIds required").into_response();
    }
    // Pool gate: when picked it must exist and be enabled; its max_concurrency
    // bounds parallel execution.
    let mut concurrency = 4usize;
    let explicit_pool = b.pool_id.as_deref().filter(|s| !s.trim().is_empty());
    if let Some(pid) = explicit_pool {
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
    // No explicit pool: auto-pick once for the whole batch (None = local).
    let auto_pool = match explicit_pool {
        Some(_) => None,
        None => st.runner.auto_pool(&b.project_id).await,
    };
    let pool_owned = explicit_pool.map(str::to_string).or(auto_pool.map(|(id, _)| id));
    let pool_id = pool_owned.as_deref();
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
                                env_id.as_deref(),
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
                    .run_union_member(
                        &report_id,
                        sid,
                        &b.project_id,
                        nodes,
                        *count,
                        &env,
                        pool_id,
                        env_id.as_deref(),
                    )
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
        let overall = if success == b.scenario_ids.len() { "SUCCESS" } else { "ERROR" };
        if st.reports.set_status(&report_id, overall).await.is_ok() {
            if let Some(notifier) = &st.runner.notifier {
                notify_scenario_result(
                    &st.runner.pool,
                    notifier,
                    ScenarioNoticeTarget::Batch(name),
                    &b.project_id,
                    &report_id,
                    overall,
                    &user.user_id,
                )
                .await;
            }
        }
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
            let trigger = user.user_id.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let env_id = effective_env(&runner, override_env.as_deref(), &sid).await;
                let out = runner
                    .run(&sid, &project, env_id.as_deref(), false, pool.as_deref(), &trigger)
                    .await;
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
            match st
                .runner
                .run(sid, &b.project_id, env_id.as_deref(), false, pool_id, &user.user_id)
                .await
            {
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
    /// Owning scenario (set only on the public share read) so the shared page can render the step tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    scenario_id: Option<String>,
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
/// Builds the report DTO by id. Shared by the authed and public (share-token) handlers.
/// Returns Ok(None) when the report doesn't exist, Err(()) on a storage error.
async fn build_scenario_report(
    st: &RunState,
    report_id: String,
) -> Result<Option<ScenarioReportResponse>, ()> {
    let (name, scenario_id) = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT COALESCE(report.name, ''), (
             SELECT execution.scenario_id FROM ms_api_scenario_execution execution
             WHERE execution.report_id = report.id ORDER BY execution.created_at DESC LIMIT 1
         ) FROM ms_api_batch_report report WHERE report.id = $1",
    )
    .bind(&report_id)
    .fetch_optional(&st.runner.pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_default();
    match st.reports.detail(&report_id).await {
        Ok(Some(d)) => Ok(Some(ScenarioReportResponse {
            report_id,
            status: d.status,
            case_count: d.case_count,
            name,
            started_at: d.started_at,
            finished_at: d.finished_at,
            duration_ms: d.duration_ms,
            scenario_id,
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
        })),
        Ok(None) => Ok(None),
        Err(_) => Err(()),
    }
}

async fn scenario_report(
    _user: AuthUser,
    State(st): State<RunState>,
    Path(report_id): Path<String>,
) -> Response {
    match build_scenario_report(&st, report_id).await {
        Ok(Some(r)) => (StatusCode::OK, Json(r)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "report not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ShareResponse {
    token: String,
    expires_at: i64,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ShareBody {
    expires_in_days: Option<i32>,
}

/// Create a public share link for a scenario report. Requires auth; the returned token maps
/// to this report and can then be read anonymously via `/public/scenario-report/{token}`.
#[utoipa::path(post, path = "/api/scenario-report/{report_id}/share", tag = "api-scenario", params(("report_id" = String, Path)), request_body = ShareBody, responses((status = 200, body = ShareResponse), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn create_scenario_share(
    user: AuthUser,
    State(st): State<RunState>,
    Path(report_id): Path<String>,
    body: Option<Json<ShareBody>>,
) -> Response {
    let days = body.and_then(|body| body.0.expires_in_days).unwrap_or(7);
    if !(1..=365).contains(&days) {
        return (StatusCode::BAD_REQUEST, "Share validity must be between 1 and 365 days")
            .into_response();
    }
    let scenario_id = match build_scenario_report(&st, report_id.clone()).await {
        Ok(Some(report)) => report.scenario_id,
        Ok(None) => return (StatusCode::NOT_FOUND, "report not found").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    };
    let share = sqlx::query_as::<_, (String, i64)>(
        "INSERT INTO ms_report_share (token, report_type, report_id, scenario_id, created_by, created_at, expires_at)
         SELECT gen_random_uuid()::text, 'scenario', $1, $2, $3, ts, ts + $4
         FROM (SELECT (extract(epoch from now()) * 1000)::bigint AS ts) clock
         RETURNING token, expires_at",
    )
    .bind(&report_id)
    .bind(scenario_id.as_ref())
    .bind(&user.user_id)
    .bind(i64::from(days) * 86_400_000)
    .fetch_one(&st.runner.pool)
    .await;
    match share {
        Ok((token, expires_at)) => {
            (StatusCode::OK, Json(ShareResponse { token, expires_at })).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

/// Public (unauthenticated) read of a shared scenario report by token.
#[utoipa::path(get, path = "/public/scenario-report/{token}", tag = "api-scenario", params(("token" = String, Path)), responses((status = 200, body = ScenarioReportResponse), (status = 404)))]
async fn public_scenario_report(State(st): State<RunState>, Path(token): Path<String>) -> Response {
    let row = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT report_id, scenario_id FROM ms_report_share \
         WHERE token = $1 AND report_type = 'scenario' AND revoked = FALSE \
           AND (expires_at IS NULL OR expires_at > (extract(epoch from now())*1000)::bigint)",
    )
    .bind(&token)
    .fetch_optional(&st.runner.pool)
    .await
    .ok()
    .flatten();
    let Some((report_id, scenario_id)) = row else {
        return (StatusCode::NOT_FOUND, "share not found or expired").into_response();
    };
    match build_scenario_report(&st, report_id).await {
        Ok(Some(mut r)) => {
            r.scenario_id = scenario_id;
            ([(axum::http::header::CACHE_CONTROL, "no-store")], Json(r)).into_response()
        }
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
    /// Remote execution location (explicit or auto-picked pool); null = local.
    executed_on: Option<ExecutedOn>,
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
                    timeout_ms: None,
                    method: method_of(&r.method),
                    url,
                    headers,
                    body: r.body.clone(),
                },
                assertions: serde_json::from_value::<Vec<Assertion>>(r.assertions.clone())
                    .unwrap_or_default(),
                processors: api_runner::domain::preparation::decode_step_processors(
                    r.processors.clone(),
                )
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
        PlanStep::Script(payload) => PlanNode::Plugin {
            label: format!(
                "SCRIPT {}",
                payload.get("output").and_then(serde_json::Value::as_str).unwrap_or("")
            ),
            processor: serde_json::from_value(
                serde_json::json!({"type":"SandboxScript","args":payload}),
            )
            .ok(),
        },
        PlanStep::Plugin(step) => PlanNode::Plugin {
            label: step
                .as_ref()
                .map_or_else(|| "PLUGIN".into(), |step| format!("PLUGIN {}", step.output)),
            processor: step.as_ref().map(|step| api_runner::Processor::Sql {
                name: step.output.clone(),
                datasource: step.profile_id.clone(),
                sql: step.query.clone(),
            }),
        },
    }
}

/// Static leaf count: a loop body counts once (not multiplied by `times`).
pub(crate) fn count_leaves(nodes: &[PlanNode]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            PlanNode::Leaf(_) | PlanNode::Plugin { .. } => 1,
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
    State(mut st): State<RunState>,
    Path(id): Path<String>,
    Json(req): Json<RunScenarioBody>,
) -> Response {
    if !user.can("API_SCENARIO", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    st.runner.actor = Some(user.clone());
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
                &user.user_id,
            )
            .await
        {
            Ok(started) => {
                let report_id = started.report_id.clone();
                let steps = started.step_ids.clone();
                let case_count = started.case_count;
                // Dispatch decision at start; the run may still fall back local.
                let executed_on = started
                    .executed_on
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                tokio::spawn(started.fut);
                (
                    StatusCode::OK,
                    Json(RunScenarioResponse {
                        report_id,
                        status: "RUNNING".to_string(),
                        case_count,
                        steps,
                        executed_on,
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
            &user.user_id,
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
                executed_on: o.executed_on,
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
    paths(run_scenario, batch_run_scenarios, build_scenario_report, create_scenario_share, public_scenario_report),
    components(schemas(RunScenarioBody, RunScenarioResponse, BatchRunBody, BatchRunResponse, BatchRunItem, ExecutedOn, ScenarioReportResponse, ReportResultItem, ReqInfo, ShareBody, ShareResponse)),
    tags((name = "api-scenario", description = "Scenarios"))
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
