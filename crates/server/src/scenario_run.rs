//! 组装根桥:`POST /api/scenario/{id}/run`。
//!
//! 把「场景编译成计划树」(api-scenario)与「计划树执行」(api-test PlanExecutor)在组装根
//! 接起来:编译保留控制器层级 → 转成执行器节点 → 解析环境 → 外层补建批量报告 → 执行 →
//! 回写报告状态 + 落场景执行记录。两个有界上下文互不依赖,跨域协调只发生在这里。
//!
//! 计划树执行是顺序、带运行上下文(变量跨步传递)的本地执行,不经资源池;失败策略
//! `failureStrategy=STOP` 时任一步失败即停后续。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use api_runner::{Assertion, HttpMethod, MatchCondition, RequestSpec};
use api_scenario::application::{
    CompileError, CompileScenarioUseCase, RecordScenarioExecutionUseCase,
};
use api_scenario::domain::PlanStep;
use api_test::adapters::plan::{Condition, Leaf, PlanExecutor, PlanNode};
use api_test::adapters::PgBatchReport;
use api_test::ports::EnvironmentPort;
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct RunState {
    compile: CompileScenarioUseCase,
    executor: PlanExecutor,
    envs: Arc<dyn EnvironmentPort>,
    reports: PgBatchReport,
    recorder: RecordScenarioExecutionUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<RunState> for Arc<dyn SessionStore> {
    fn from_ref(s: &RunState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    compile: CompileScenarioUseCase,
    executor: PlanExecutor,
    envs: Arc<dyn EnvironmentPort>,
    reports: PgBatchReport,
    recorder: RecordScenarioExecutionUseCase,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new().route("/api/scenario/{id}/run", post(run_scenario)).with_state(RunState {
        compile,
        executor,
        envs,
        reports,
        recorder,
        sessions,
    })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunScenarioBody {
    project_id: String,
    /// 运行所用环境 id(注入 base_url/默认头/变量);缺省不注入。
    #[serde(default)]
    environment_id: Option<String>,
    /// 失败策略:CONTINUE(默认)跑完全部;STOP 任一步失败即停。
    #[serde(default = "default_strategy")]
    failure_strategy: String,
}

fn default_strategy() -> String {
    "CONTINUE".to_string()
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunScenarioResponse {
    report_id: String,
    status: String,
    case_count: usize,
}

// ---- 计划树:api-scenario 中立树 → api-test 执行器节点 ----

/// 操作符串(EQUALS/CONTAINS/…)→ MatchCondition;无法识别回落 Equals。
fn op_to_condition(op: &str) -> MatchCondition {
    serde_json::from_value(serde_json::Value::String(op.to_uppercase()))
        .unwrap_or(MatchCondition::Equals)
}

fn method_of(m: &str) -> HttpMethod {
    serde_json::from_value(serde_json::Value::String(m.to_uppercase())).unwrap_or(HttpMethod::Get)
}

fn to_nodes(steps: &[PlanStep], once: &mut u32) -> Vec<PlanNode> {
    steps.iter().map(|s| to_node(s, once)).collect()
}

fn to_node(step: &PlanStep, once: &mut u32) -> PlanNode {
    match step {
        PlanStep::Case(id) => PlanNode::Leaf(Leaf::Case { case_id: id.clone() }),
        PlanStep::Request(r) => PlanNode::Leaf(Leaf::Request {
            label: format!("{} {}", r.method, r.url),
            request: RequestSpec {
                method: method_of(&r.method),
                url: r.url.clone(),
                headers: vec![],
                body: r.body.clone(),
            },
            // 中立 JSON 断言 → 具体 Assertion(形态不符则回落空,不致命),执行器据此判定。
            assertions: serde_json::from_value::<Vec<Assertion>>(r.assertions.clone())
                .unwrap_or_default(),
            processors: vec![],
        }),
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

/// 静态叶子数(循环体只算一次),作场景执行记录的 case_count。
fn count_leaves(nodes: &[PlanNode]) -> usize {
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
    // 1) 编译为计划树(保留控制器层级;环/深度在用例内拦)
    let plan = match st.compile.compile_plan(&id).await {
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
    // 2) 转执行器节点;无可执行叶子即 400
    let mut once = 0u32;
    let nodes = to_nodes(&plan, &mut once);
    let count = count_leaves(&nodes);
    if count == 0 {
        return (StatusCode::BAD_REQUEST, "scenario has no runnable steps").into_response();
    }
    // 3) 解析运行环境(给定 environmentId 才注入)
    let env = match req.environment_id.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(eid) => match st.envs.resolve(eid).await {
            Ok(e) => e.unwrap_or_default(),
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "env resolve error").into_response(),
        },
        None => Default::default(),
    };
    let stop_on_failure = req.failure_strategy.eq_ignore_ascii_case("STOP");

    // 4) 外层补建批量报告(RUNNING),拿 report_id 给用例结果归组
    let report_id = match st.reports.create("SERIAL", count as i32).await {
        Ok(r) => r,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "create report error").into_response(),
    };

    // 5) 走计划树执行
    let all_pass = match st.executor.run(&report_id, &nodes, &env, stop_on_failure).await {
        Ok(p) => p,
        Err(_) => {
            let _ = st.reports.set_status(&report_id, "ERROR").await;
            return (StatusCode::INTERNAL_SERVER_ERROR, "execution error").into_response();
        }
    };
    let status = if all_pass { "SUCCESS" } else { "ERROR" };

    // 6) 回写报告状态 + 落场景执行记录(记录失败不影响运行结果)
    let _ = st.reports.set_status(&report_id, status).await;
    let _ = st
        .recorder
        .execute(&id, &req.project_id, status, count as i32, Some(&report_id))
        .await;

    (
        StatusCode::OK,
        Json(RunScenarioResponse {
            report_id,
            status: status.to_string(),
            case_count: count,
        }),
    )
        .into_response()
}

#[derive(OpenApi)]
#[openapi(
    paths(run_scenario),
    components(schemas(RunScenarioBody, RunScenarioResponse)),
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
        // 场景内联请求的中立 JSON 断言 → 执行器 Leaf::Request 的具体 Assertion。
        let req = InlineRequest::new("GET", "http://x/ok", None)
            .expect("valid")
            .with_assertions(serde_json::json!([
                {"type": "StatusIs", "args": 200},
                {"type": "BodyContains", "args": "ok"}
            ]));
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
