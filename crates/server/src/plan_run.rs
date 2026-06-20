//! 组装根桥:`POST /test-plan/{id}/run` —— 执行计划内挂入的用例并**自动回写结果**。
//!
//! 逐条:解析用例规格(ms_api_case)→ 用 `ReqwestRunner` 就地执行 → 结构化断言(evaluate_detailed)
//! → 回写到 `ms_test_plan_case`(状态 + 耗时 + 状态码 + 响应体 + 断言表)。
//! 跑完报告即真实数据,无需手填。RBAC 资源键 `TEST_PLAN:EXECUTE`。

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

use api_runner::{evaluate_detailed, CaseOutcome, ReqwestRunner};
use api_test::adapters::local::CaseSpecSource;
use api_test::adapters::pg::PgCaseSpecSource;
use test_plan::application::PlanCaseUseCase;
use test_plan::domain::{AssertionResult, CaseResult, CaseStatus};

#[derive(Clone)]
struct RunState {
    cases: PlanCaseUseCase,
    specs: Arc<PgCaseSpecSource>,
    runner: Arc<ReqwestRunner>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<RunState> for Arc<dyn SessionStore> {
    fn from_ref(s: &RunState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(pool: PgPool, sessions: Arc<dyn SessionStore>) -> Router {
    let plan_repo = Arc::new(test_plan::adapters::pg::PgPlanRepository::new(pool.clone()));
    Router::new().route("/test-plan/{id}/run", post(run_plan)).with_state(RunState {
        cases: PlanCaseUseCase::new(plan_repo),
        specs: Arc::new(PgCaseSpecSource::new(pool)),
        // no_proxy:直连被测主机,不被全局代理劫持。
        runner: Arc::new(ReqwestRunner::no_proxy()),
        sessions,
    })
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
    let cases = match st.cases.list(&id).await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    };
    let total = cases.len();
    let (mut executed, mut success, mut failed) = (0usize, 0usize, 0usize);

    for pc in &cases {
        // 解析用例规格(ms_api_case)。取不到 → 记 BLOCK(无法执行)。
        let spec = match st.specs.spec_of(&pc.case_id).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                let _ = st
                    .cases
                    .record(
                        &id,
                        &pc.case_id,
                        CaseStatus::Block,
                        Some(CaseResult {
                            body: Some("用例规格未找到(非 ms_api_case)".into()),
                            ..Default::default()
                        }),
                    )
                    .await;
                continue;
            }
            Err(_) => continue,
        };

        let (report, snap) =
            st.runner.run_case_with_snapshot(&spec.request, &spec.assertions).await;
        executed += 1;
        let status = match report.outcome {
            CaseOutcome::Success => {
                success += 1;
                CaseStatus::Success
            }
            CaseOutcome::Error => {
                failed += 1;
                CaseStatus::Error
            }
        };

        let result = match &snap {
            Some(s) => CaseResult {
                latency_ms: s.elapsed_ms,
                response_size: s.body.len() as u64,
                status_code: Some(s.status as i64),
                body: Some(truncate(&s.body, 2000)),
                assertions: evaluate_detailed(&spec.assertions, s)
                    .into_iter()
                    .map(|a| AssertionResult {
                        item: a.item,
                        actual: a.actual,
                        condition: a.condition,
                        expected: a.expected,
                        passed: a.passed,
                        reason: a.reason,
                    })
                    .collect(),
            },
            // 传输失败(连不上):无快照,把失败原因放进响应体。
            None => CaseResult {
                body: Some(report.failures.join("; ")),
                ..Default::default()
            },
        };
        let _ = st.cases.record(&id, &pc.case_id, status, Some(result)).await;
    }

    (
        StatusCode::OK,
        Json(RunPlanResponse { plan_id: id, total, executed, success, failed }),
    )
        .into_response()
}

#[derive(OpenApi)]
#[openapi(paths(run_plan), components(schemas(RunPlanResponse)), tags((name = "test-plan", description = "测试计划")))]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
