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
use test_plan::application::PlanCaseUseCase;
use test_plan::domain::{AssertionResult, CaseResult, CaseStatus, RequestInfo};

use crate::scenario_run::{RunError, ScenarioRunner};

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
    pub fn new(pool: PgPool) -> Self {
        let plan_repo = Arc::new(test_plan::adapters::pg::PgPlanRepository::new(pool.clone()));
        let scenario_repo =
            Arc::new(api_scenario::adapters::pg::PgApiScenarioRepository::new(pool.clone()));
        // Same wiring as main.rs: scenario-mounted plan entries run through the
        // one true scenario executor (cross-step vars, seeds, scenario report).
        let scenarios = ScenarioRunner {
            compile: api_scenario::application::CompileScenarioUseCase::new(scenario_repo.clone()),
            executor: PlanExecutor::new(
                Arc::new(PgCaseSpecSource::new(pool.clone())),
                Arc::new(PgCaseResultSink::new(pool.clone())),
            ),
            envs: Arc::new(PgEnvironment::new(pool.clone())),
            reports: PgBatchReport::new(pool.clone()),
            recorder: api_scenario::application::RecordScenarioExecutionUseCase::new(scenario_repo),
            pool: pool.clone(),
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

    pub async fn run(&self, plan_id: &str, env_id: Option<&str>) -> Result<RunSummary, ()> {
        let env_id = env_id.filter(|s| !s.trim().is_empty());
        let env: Option<ResolvedEnv> = match env_id {
            Some(id) => self.envs.resolve(id).await.ok().flatten(),
            None => None,
        };
        let env = env.as_ref();
        let cases = self.cases.list(plan_id).await.map_err(|_| ())?;
        let project_id = self.project_of(plan_id).await;
        let total = cases.len();
        let (mut executed, mut success, mut failed) = (0usize, 0usize, 0usize);
        for pc in &cases {
            if let Ok(Some(spec)) = self.specs.spec_of(&pc.case_id).await {
                let (status, result) =
                    run_request(&self.runner, &spec.request, &spec.assertions, env).await;
                executed += 1;
                match status {
                    CaseStatus::Success => success += 1,
                    _ => failed += 1,
                }
                let _ = self.cases.record(plan_id, &pc.case_id, status, Some(result)).await;
                continue;
            }
            // Not an API case: treat as a scenario-mounted entry. The run-level
            // env wins; otherwise the scenario's own configured environment.
            let eff_env = match env_id {
                Some(id) => Some(id.to_string()),
                None => self.scenarios.default_env_of(&pc.case_id).await,
            };
            match self.scenarios.run(&pc.case_id, &project_id, eff_env.as_deref(), false).await {
                Ok(o) => {
                    let ok = o.status == "SUCCESS";
                    let status = if ok { CaseStatus::Success } else { CaseStatus::Error };
                    executed += 1;
                    if ok {
                        success += 1;
                    } else {
                        failed += 1;
                    }
                    let result = CaseResult { report_id: Some(o.report_id), ..Default::default() };
                    let _ = self.cases.record(plan_id, &pc.case_id, status, Some(result)).await;
                }
                Err(RunError::NotFound) => {
                    let _ = self
                        .cases
                        .record(
                            plan_id,
                            &pc.case_id,
                            CaseStatus::Block,
                            Some(CaseResult {
                                body: Some("用例规格未找到(非 ms_api_case / 非场景)".into()),
                                ..Default::default()
                            }),
                        )
                        .await;
                }
                Err(e) => {
                    executed += 1;
                    failed += 1;
                    let msg = match e {
                        RunError::CycleOrDepth => "场景引用成环或过深",
                        RunError::NoSteps => "场景无可执行步骤",
                        _ => "场景执行失败",
                    };
                    let _ = self
                        .cases
                        .record(
                            plan_id,
                            &pc.case_id,
                            CaseStatus::Error,
                            Some(CaseResult { body: Some(msg.into()), ..Default::default() }),
                        )
                        .await;
                }
            }
        }
        Ok(RunSummary { total, executed, success, failed })
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

pub fn router(pool: PgPool, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/test-plan/{id}/run", post(run_plan))
        .route("/test-plan/by-case/{caseId}", get(plans_by_case))
        .with_state(RunState { plan_runner: PlanRunner::new(pool.clone()), pool, sessions })
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
    match st.plan_runner.run(&id, body.environment_id.as_deref()).await {
        Ok(s) => (
            StatusCode::OK,
            Json(RunPlanResponse {
                plan_id: id,
                total: s.total,
                executed: s.executed,
                success: s.success,
                failed: s.failed,
            }),
        )
            .into_response(),
        Err(()) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(paths(run_plan), components(schemas(RunPlanResponse, RunPlanBody)), tags((name = "test-plan", description = "测试计划")))]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
