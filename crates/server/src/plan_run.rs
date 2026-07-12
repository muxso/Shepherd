use std::future::Future;
use std::pin::Pin;
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

use api_runner::{
    evaluate_detailed, Assertion, CaseOutcome, HttpMethod, RequestSpec, ReqwestRunner,
};
use api_scenario::application::CompileScenarioUseCase;
use api_scenario::domain::PlanStep;
use api_test::adapters::local::{apply_env_static, CaseSpecSource};
use api_test::adapters::pg::{PgCaseSpecSource, PgEnvironment};
use api_test::domain::ResolvedEnv;
use api_test::ports::EnvironmentPort;
use test_plan::application::PlanCaseUseCase;
use test_plan::domain::{AssertionResult, CaseResult, CaseStatus, RequestInfo, StepResult};

#[derive(Clone)]
pub struct PlanRunner {
    cases: PlanCaseUseCase,
    specs: Arc<PgCaseSpecSource>,
    compile: CompileScenarioUseCase,
    runner: Arc<ReqwestRunner>,
    envs: Arc<PgEnvironment>,
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
        Self {
            cases: PlanCaseUseCase::new(plan_repo),
            specs: Arc::new(PgCaseSpecSource::new(pool.clone())),
            compile: CompileScenarioUseCase::new(scenario_repo),
            runner: Arc::new(ReqwestRunner::no_proxy()),
            envs: Arc::new(PgEnvironment::new(pool)),
        }
    }

    pub async fn run(&self, plan_id: &str, env_id: Option<&str>) -> Result<RunSummary, ()> {
        let env: Option<ResolvedEnv> = match env_id.filter(|s| !s.trim().is_empty()) {
            Some(id) => self.envs.resolve(id).await.ok().flatten(),
            None => None,
        };
        let env = env.as_ref();
        let cases = self.cases.list(plan_id).await.map_err(|_| ())?;
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
            if let Ok(steps_plan) = self.compile.compile_plan(&pc.case_id).await {
                let steps = run_steps(&steps_plan, &self.runner, &self.specs, env).await;
                let ok = !steps.is_empty() && steps.iter().all(|s| s.status == CaseStatus::Success);
                let status = if ok { CaseStatus::Success } else { CaseStatus::Error };
                executed += 1;
                if ok {
                    success += 1;
                } else {
                    failed += 1;
                }
                let result = CaseResult {
                    latency_ms: steps.iter().map(|s| s.latency_ms).sum(),
                    steps,
                    ..Default::default()
                };
                let _ = self.cases.record(plan_id, &pc.case_id, status, Some(result)).await;
                continue;
            }
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

fn method_of(m: &str) -> HttpMethod {
    serde_json::from_value(serde_json::Value::String(m.to_uppercase())).unwrap_or(HttpMethod::Get)
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
            steps: vec![],
        },
        None => CaseResult {
            body: Some(report.failures.join("; ")),
            request: Some(req_info),
            ..Default::default()
        },
    };
    (status, result)
}

fn run_steps<'a>(
    steps: &'a [PlanStep],
    runner: &'a ReqwestRunner,
    specs: &'a PgCaseSpecSource,
    env: Option<&'a ResolvedEnv>,
) -> Pin<Box<dyn Future<Output = Vec<StepResult>> + Send + 'a>> {
    Box::pin(async move {
        let mut out = Vec::new();
        for step in steps {
            out.push(run_step(step, runner, specs, env).await);
        }
        out
    })
}

async fn run_step(
    step: &PlanStep,
    runner: &ReqwestRunner,
    specs: &PgCaseSpecSource,
    env: Option<&ResolvedEnv>,
) -> StepResult {
    let control = |name: String, kind: &str, children: Vec<StepResult>| {
        let ok = children.iter().all(|c| c.status == CaseStatus::Success);
        let latency = children.iter().map(|c| c.latency_ms).sum();
        StepResult {
            name,
            kind: kind.to_string(),
            status: if ok { CaseStatus::Success } else { CaseStatus::Error },
            latency_ms: latency,
            status_code: None,
            assertions: vec![],
            children,
        }
    };
    match step {
        PlanStep::Case(case_id) => match specs.spec_of(case_id).await {
            Ok(Some(spec)) => {
                let (status, r) = run_request(runner, &spec.request, &spec.assertions, env).await;
                StepResult {
                    name: format!("{} {}", spec.request.method.as_str(), spec.request.url),
                    kind: "接口用例".to_string(),
                    status,
                    latency_ms: r.latency_ms,
                    status_code: r.status_code,
                    assertions: r.assertions,
                    children: vec![],
                }
            }
            _ => StepResult {
                name: format!("用例 {case_id}"),
                kind: "接口用例".to_string(),
                status: CaseStatus::Block,
                latency_ms: 0,
                status_code: None,
                assertions: vec![],
                children: vec![],
            },
        },
        PlanStep::Request(req) => {
            let spec = RequestSpec {
                method: method_of(&req.method),
                url: req.url.clone(),
                headers: vec![],
                body: req.body.clone(),
            };
            let assertions: Vec<Assertion> =
                serde_json::from_value(req.assertions.clone()).unwrap_or_default();
            let (status, r) = run_request(runner, &spec, &assertions, env).await;
            StepResult {
                name: format!("{} {}", req.method, req.url),
                kind: "请求".to_string(),
                status,
                latency_ms: r.latency_ms,
                status_code: r.status_code,
                assertions: r.assertions,
                children: vec![],
            }
        }
        PlanStep::Loop { times, body } => control(
            format!("循环 x{times}"),
            "循环控制器",
            run_steps(body, runner, specs, env).await,
        ),
        PlanStep::If { variable, operator, value, body } => control(
            format!("若 {variable} {operator} {value}"),
            "条件控制器",
            run_steps(body, runner, specs, env).await,
        ),
        PlanStep::Once { body } => {
            control("仅一次".to_string(), "ONCE 控制器", run_steps(body, runner, specs, env).await)
        }
        PlanStep::Timer { ms } => StepResult {
            name: format!("等待 {ms}ms"),
            kind: "等待".to_string(),
            status: CaseStatus::Success,
            latency_ms: *ms,
            status_code: None,
            assertions: vec![],
            children: vec![],
        },
    }
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
