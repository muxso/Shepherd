use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use migrate::PgPool;
use serde::Serialize;
use sqlx::Row;
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use api_runner::{evaluate_detailed, Assertion, CaseOutcome, RequestSpec, ReqwestRunner};
use api_test::adapters::local::{apply_env_static, CaseSpecSource};
use api_test::adapters::pg::{PgCaseResultSink, PgCaseSpecSource, PgEnvironment};
use api_test::adapters::plan::PlanExecutor;
use api_test::adapters::PgBatchReport;
use api_test::domain::ResolvedEnv;
use api_test::ports::EnvironmentPort;
use test_plan::adapters::pg::{PgPlanRepository, PgScheduleStore};
use test_plan::application::{PlanCaseUseCase, PlanStatisticsUseCase, ScheduledRunUseCase};
use test_plan::domain::{AssertionResult, CaseResult, CaseStatus, RequestInfo};

use crate::pool_runner_ws::{HubObserver, PoolHub};
use crate::scenario_run::{ExecutedOn, RunError, ScenarioRunner};

#[derive(Clone)]
pub struct PlanRunner {
    cases: PlanCaseUseCase,
    specs: Arc<PgCaseSpecSource>,
    runner: Arc<ReqwestRunner>,
    envs: Arc<PgEnvironment>,
    scenarios: ScenarioRunner,
    pool: PgPool,
}

pub struct RunSummary {
    pub total: usize,
    pub executed: usize,
    pub success: usize,
    pub failed: usize,
}

impl PlanRunner {
    /// `hub` enables pool routing + live run events for scenario-mounted
    /// entries; None keeps plan runs fully in-process (MCP, scheduler).
    pub fn new(pool: PgPool, hub: Option<Arc<PoolHub>>) -> Self {
        let plan_repo = Arc::new(test_plan::adapters::pg::PgPlanRepository::new(pool.clone()));
        let scenario_repo =
            Arc::new(api_scenario::adapters::pg::PgApiScenarioRepository::new(pool.clone()));
        // Same wiring as main.rs: scenario-mounted plan entries run through the
        // one true scenario executor (cross-step vars, seeds, scenario report).
        let mut executor = PlanExecutor::new(
            Arc::new(PgCaseSpecSource::new(pool.clone())),
            Arc::new(PgCaseResultSink::new(pool.clone())),
        );
        if let Some(h) = &hub {
            executor = executor.with_observer(Arc::new(HubObserver::new(h.events())));
        }
        let scenarios = ScenarioRunner {
            compile: api_scenario::application::CompileScenarioUseCase::new(scenario_repo.clone()),
            executor,
            envs: Arc::new(PgEnvironment::new(pool.clone())),
            reports: PgBatchReport::new(pool.clone()),
            recorder: api_scenario::application::RecordScenarioExecutionUseCase::new(scenario_repo),
            pool: pool.clone(),
            specs: Arc::new(PgCaseSpecSource::new(pool.clone())),
            hub,
        };
        Self {
            cases: PlanCaseUseCase::new(plan_repo),
            specs: Arc::new(PgCaseSpecSource::new(pool.clone())),
            runner: Arc::new(ReqwestRunner::no_proxy()),
            envs: Arc::new(PgEnvironment::new(pool.clone())),
            scenarios,
            pool,
        }
    }

    /// Owning project of the plan; scenario execution records are filed under it.
    async fn project_of(&self, plan_id: &str) -> String {
        sqlx::query_scalar::<_, String>("SELECT project_id FROM ms_test_plan WHERE id = $1")
            .bind(plan_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Executes one linked entry (API case or scenario) and records its result.
    /// `counted` is false for the spec-missing BLOCK case, which run() leaves
    /// out of the executed/failed tally. The last element is where a scenario
    /// entry executed remotely (None = local / plain API case).
    async fn execute_case(
        &self,
        plan_id: &str,
        project_id: &str,
        case_id: &str,
        env_id: Option<&str>,
        env: Option<&ResolvedEnv>,
        pool_id: Option<&str>,
    ) -> (CaseStatus, bool, Option<ExecutedOn>) {
        if let Ok(Some(spec)) = self.specs.spec_of(case_id).await {
            let (status, result) =
                run_request(&self.runner, &spec.request, &spec.assertions, env).await;
            let _ = self.cases.record(plan_id, case_id, status, Some(result)).await;
            return (status, true, None);
        }
        // Not an API case: treat as a scenario-mounted entry. The run-level
        // env wins; otherwise the scenario's own configured environment.
        let eff_env = match env_id {
            Some(id) => Some(id.to_string()),
            None => self.scenarios.default_env_of(case_id).await,
        };
        match self.scenarios.run(case_id, project_id, eff_env.as_deref(), false, pool_id).await {
            Ok(o) => {
                let ok = o.status == "SUCCESS";
                let status = if ok { CaseStatus::Success } else { CaseStatus::Error };
                let result = CaseResult { report_id: Some(o.report_id), ..Default::default() };
                let _ = self.cases.record(plan_id, case_id, status, Some(result)).await;
                (status, true, o.executed_on)
            }
            Err(RunError::NotFound) => {
                let _ = self
                    .cases
                    .record(
                        plan_id,
                        case_id,
                        CaseStatus::Block,
                        Some(CaseResult {
                            body: Some("用例规格未找到(非 ms_api_case / 非场景)".into()),
                            ..Default::default()
                        }),
                    )
                    .await;
                (CaseStatus::Block, false, None)
            }
            Err(e) => {
                let msg = match e {
                    RunError::CycleOrDepth => "场景引用成环或过深",
                    RunError::NoSteps => "场景无可执行步骤",
                    _ => "场景执行失败",
                };
                let _ = self
                    .cases
                    .record(
                        plan_id,
                        case_id,
                        CaseStatus::Error,
                        Some(CaseResult { body: Some(msg.into()), ..Default::default() }),
                    )
                    .await;
                (CaseStatus::Error, true, None)
            }
        }
    }

    async fn resolve_env(&self, env_id: Option<&str>) -> Option<ResolvedEnv> {
        match env_id {
            Some(id) => self.envs.resolve(id).await.ok().flatten(),
            None => None,
        }
    }

    pub async fn run(
        &self,
        plan_id: &str,
        env_id: Option<&str>,
        pool_id: Option<&str>,
    ) -> Result<RunSummary, ()> {
        let env_id = env_id.filter(|s| !s.trim().is_empty());
        let pool_id = pool_id.filter(|s| !s.trim().is_empty());
        let env = self.resolve_env(env_id).await;
        let cases = self.cases.list(plan_id).await.map_err(|_| ())?;
        let project_id = self.project_of(plan_id).await;
        let total = cases.len();
        let (mut executed, mut success, mut failed) = (0usize, 0usize, 0usize);
        for pc in &cases {
            let (status, counted, _) = self
                .execute_case(plan_id, &project_id, &pc.case_id, env_id, env.as_ref(), pool_id)
                .await;
            if counted {
                executed += 1;
                match status {
                    CaseStatus::Success => success += 1,
                    _ => failed += 1,
                }
            }
        }
        Ok(RunSummary { total, executed, success, failed })
    }

    /// Runs exactly one linked case/scenario; None when the case is not linked.
    pub async fn run_case(
        &self,
        plan_id: &str,
        case_id: &str,
        env_id: Option<&str>,
        pool_id: Option<&str>,
    ) -> Result<Option<(CaseStatus, Option<ExecutedOn>)>, ()> {
        let env_id = env_id.filter(|s| !s.trim().is_empty());
        let pool_id = pool_id.filter(|s| !s.trim().is_empty());
        let cases = self.cases.list(plan_id).await.map_err(|_| ())?;
        if !cases.iter().any(|c| c.case_id == case_id) {
            return Ok(None);
        }
        let env = self.resolve_env(env_id).await;
        let project_id = self.project_of(plan_id).await;
        let (status, _, executed_on) =
            self.execute_case(plan_id, &project_id, case_id, env_id, env.as_ref(), pool_id).await;
        Ok(Some((status, executed_on)))
    }
}

#[derive(Clone)]
struct RunState {
    plan_runner: PlanRunner,
    pool: PgPool,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<RunState> for Arc<dyn SessionStore> {
    fn from_ref(s: &RunState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(pool: PgPool, sessions: Arc<dyn SessionStore>, hub: Option<Arc<PoolHub>>) -> Router {
    Router::new()
        .route("/test-plan/{id}/run", post(run_plan))
        .route("/test-plan/{id}/cases/{caseId}/run", post(run_plan_case))
        .route("/test-plan/by-case/{caseId}", get(plans_by_case))
        .with_state(RunState { plan_runner: PlanRunner::new(pool.clone(), hub), pool, sessions })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanRef {
    plan_id: String,
    name: String,
}

async fn plans_by_case(State(st): State<RunState>, Path(case_id): Path<String>) -> Response {
    let rows = sqlx::query(
        "SELECT p.id AS id, p.name AS name FROM ms_test_plan_case tc \
         JOIN ms_test_plan p ON p.id = tc.plan_id WHERE tc.case_id = $1 ORDER BY p.name",
    )
    .bind(&case_id)
    .fetch_all(&st.pool)
    .await;
    match rows {
        Ok(rows) => {
            let out: Vec<PlanRef> = rows
                .iter()
                .map(|r| PlanRef {
                    plan_id: r.get::<String, _>("id"),
                    name: r.get::<String, _>("name"),
                })
                .collect();
            (StatusCode::OK, Json(out)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunPlanResponse {
    plan_id: String,
    total: usize,
    executed: usize,
    success: usize,
    failed: usize,
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

fn map_assertions(reps: Vec<api_runner::AssertionReport>) -> Vec<AssertionResult> {
    reps.into_iter()
        .map(|a| AssertionResult {
            item: a.item,
            actual: a.actual,
            condition: a.condition,
            expected: a.expected,
            passed: a.passed,
            reason: a.reason,
        })
        .collect()
}

async fn run_request(
    runner: &ReqwestRunner,
    request: &RequestSpec,
    assertions: &[Assertion],
    env: Option<&ResolvedEnv>,
) -> (CaseStatus, CaseResult) {
    let mut request = request.clone();
    if let Some(e) = env {
        apply_env_static(&mut request, e);
    }
    let request = &request;
    let (report, snap) = runner.run_case_with_snapshot(request, assertions).await;
    let status = match report.outcome {
        CaseOutcome::Success => CaseStatus::Success,
        CaseOutcome::Error => CaseStatus::Error,
    };
    let req_info = RequestInfo {
        method: request.method.as_str().to_string(),
        url: request.url.clone(),
        headers: request.headers.clone(),
        body: request.body.clone(),
    };
    let result = match &snap {
        Some(s) => CaseResult {
            latency_ms: s.elapsed_ms,
            response_size: s.body.len() as u64,
            status_code: Some(s.status as i64),
            body: Some(truncate(&s.body, 2000)),
            assertions: map_assertions(evaluate_detailed(assertions, s)),
            response_headers: s.headers.clone(),
            request: Some(req_info),
            ..Default::default()
        },
        None => CaseResult {
            body: Some(report.failures.join("; ")),
            request: Some(req_info),
            ..Default::default()
        },
    };
    (status, result)
}

#[derive(Debug, Default, serde::Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunPlanBody {
    #[serde(default)]
    environment_id: Option<String>,
    /// Resource pool routing for scenario-mounted entries: a pool with a
    /// connected runner executes them remotely; empty = in-process.
    #[serde(default)]
    pool_id: Option<String>,
}

#[utoipa::path(
    post, path = "/test-plan/{id}/run", tag = "test-plan",
    params(("id" = String, Path)),
    request_body = RunPlanBody,
    responses((status = 200, body = RunPlanResponse), (status = 403)),
    security(("bearer" = []))
)]
async fn run_plan(
    user: AuthUser,
    State(st): State<RunState>,
    Path(id): Path<String>,
    Json(body): Json<RunPlanBody>,
) -> Response {
    if !user.can("TEST_PLAN", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.plan_runner.run(&id, body.environment_id.as_deref(), body.pool_id.as_deref()).await {
        Ok(s) => {
            // Manual runs snapshot into the run history like scheduled runs do,
            // so the 执行历史 tab reflects them.
            let stats =
                PlanStatisticsUseCase::new(Arc::new(PgPlanRepository::new(st.pool.clone())));
            let run_uc =
                ScheduledRunUseCase::new(stats, Arc::new(PgScheduleStore::new(st.pool.clone())));
            if let Err(e) = run_uc.execute(&id).await {
                tracing::warn!(plan = %id, "manual run snapshot failed: {e:?}");
            }
            (
                StatusCode::OK,
                Json(RunPlanResponse {
                    plan_id: id,
                    total: s.total,
                    executed: s.executed,
                    success: s.success,
                    failed: s.failed,
                }),
            )
                .into_response()
        }
        Err(()) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunPlanCaseResponse {
    case_id: String,
    status: String,
    /// Remote execution location for scenario entries; null = local.
    executed_on: Option<ExecutedOn>,
}

#[utoipa::path(
    post, path = "/test-plan/{id}/cases/{caseId}/run", tag = "test-plan",
    params(("id" = String, Path), ("caseId" = String, Path)),
    request_body = RunPlanBody,
    responses((status = 200, body = RunPlanCaseResponse), (status = 403), (status = 404)),
    security(("bearer" = []))
)]
async fn run_plan_case(
    user: AuthUser,
    State(st): State<RunState>,
    Path((id, case_id)): Path<(String, String)>,
    body: Option<Json<RunPlanBody>>,
) -> Response {
    if !user.can("TEST_PLAN", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let body = body.map(|Json(b)| b).unwrap_or_default();
    match st
        .plan_runner
        .run_case(&id, &case_id, body.environment_id.as_deref(), body.pool_id.as_deref())
        .await
    {
        Ok(Some((status, executed_on))) => (
            StatusCode::OK,
            Json(RunPlanCaseResponse { case_id, status: status.as_str().to_string(), executed_on }),
        )
            .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "case not linked to plan").into_response(),
        Err(()) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(paths(run_plan, run_plan_case), components(schemas(RunPlanResponse, RunPlanBody, RunPlanCaseResponse)), tags((name = "test-plan", description = "测试计划")))]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
