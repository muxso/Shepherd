//! 组装根桥:`POST /test-plan/{id}/run` —— 执行计划内挂入的用例/场景并**自动回写结果**。
//!
//! - 普通用例(ms_api_case):解析规格 → `ReqwestRunner` 执行 → 断言表 + 响应头 + 实际请求。
//! - 场景(ms_api_scenario):编译成计划树 → 逐叶执行 → 组成嵌套步骤树。
//!
//! 结果回写 `ms_test_plan_case`,报告即真实数据。RBAC 资源键 `TEST_PLAN:EXECUTE`。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use migrate::PgPool;
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use api_runner::{evaluate_detailed, Assertion, CaseOutcome, HttpMethod, ReqwestRunner, RequestSpec};
use api_scenario::application::CompileScenarioUseCase;
use api_scenario::domain::PlanStep;
use api_test::adapters::local::CaseSpecSource;
use api_test::adapters::pg::PgCaseSpecSource;
use test_plan::application::PlanCaseUseCase;
use test_plan::domain::{AssertionResult, CaseResult, CaseStatus, RequestInfo, StepResult};

/// 计划执行器:跑计划内挂入的用例/场景并回写结果。HTTP 端点与定时调度共用。
#[derive(Clone)]
pub struct PlanRunner {
    cases: PlanCaseUseCase,
    specs: Arc<PgCaseSpecSource>,
    compile: CompileScenarioUseCase,
    runner: Arc<ReqwestRunner>,
}

/// 一次计划执行的汇总。
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
            specs: Arc::new(PgCaseSpecSource::new(pool)),
            compile: CompileScenarioUseCase::new(scenario_repo),
            runner: Arc::new(ReqwestRunner::no_proxy()),
        }
    }

    /// 执行计划:逐条跑挂入的用例/场景并回写结果。
    pub async fn run(&self, plan_id: &str) -> Result<RunSummary, ()> {
        let cases = self.cases.list(plan_id).await.map_err(|_| ())?;
        let total = cases.len();
        let (mut executed, mut success, mut failed) = (0usize, 0usize, 0usize);
        for pc in &cases {
            // 1) 普通用例(ms_api_case)。
            if let Ok(Some(spec)) = self.specs.spec_of(&pc.case_id).await {
                let (status, result) =
                    run_request(&self.runner, &spec.request, &spec.assertions).await;
                executed += 1;
                match status {
                    CaseStatus::Success => success += 1,
                    _ => failed += 1,
                }
                let _ = self.cases.record(plan_id, &pc.case_id, status, Some(result)).await;
                continue;
            }
            // 2) 场景(ms_api_scenario):编译树 → 逐叶执行 → 嵌套步骤。
            if let Ok(steps_plan) = self.compile.compile_plan(&pc.case_id).await {
                let steps = run_steps(&steps_plan, &self.runner, &self.specs).await;
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
            // 3) 既非用例也非场景:无法执行,记 BLOCK。
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
        .with_state(RunState { plan_runner: PlanRunner::new(pool), sessions })
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

/// 执行一个请求叶子,产出执行明细(状态/耗时/状态码/断言/响应头/响应体/实际请求)。
async fn run_request(
    runner: &ReqwestRunner,
    request: &RequestSpec,
    assertions: &[Assertion],
) -> (CaseStatus, CaseResult) {
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

/// 递归执行场景计划树,产出嵌套步骤结果。
fn run_steps<'a>(
    steps: &'a [PlanStep],
    runner: &'a ReqwestRunner,
    specs: &'a PgCaseSpecSource,
) -> Pin<Box<dyn Future<Output = Vec<StepResult>> + Send + 'a>> {
    Box::pin(async move {
        let mut out = Vec::new();
        for step in steps {
            out.push(run_step(step, runner, specs).await);
        }
        out
    })
}

async fn run_step(step: &PlanStep, runner: &ReqwestRunner, specs: &PgCaseSpecSource) -> StepResult {
    // 控制器节点:聚合子步骤状态。
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
                let (status, r) = run_request(runner, &spec.request, &spec.assertions).await;
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
            let (status, r) = run_request(runner, &spec, &assertions).await;
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
        PlanStep::Loop { times, body } => {
            control(format!("循环 x{times}"), "循环控制器", run_steps(body, runner, specs).await)
        }
        PlanStep::If { variable, operator, value, body } => control(
            format!("若 {variable} {operator} {value}"),
            "条件控制器",
            run_steps(body, runner, specs).await,
        ),
        PlanStep::Once { body } => {
            control("仅一次".to_string(), "ONCE 控制器", run_steps(body, runner, specs).await)
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

#[utoipa::path(
    post, path = "/test-plan/{id}/run", tag = "test-plan",
    params(("id" = String, Path)),
    responses((status = 200, body = RunPlanResponse), (status = 403)),
    security(("bearer" = []))
)]
async fn run_plan(user: AuthUser, State(st): State<RunState>, Path(id): Path<String>) -> Response {
    if !user.can("TEST_PLAN", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.plan_runner.run(&id).await {
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
#[openapi(paths(run_plan), components(schemas(RunPlanResponse)), tags((name = "test-plan", description = "测试计划")))]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
