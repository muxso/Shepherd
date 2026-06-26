use std::sync::Arc;

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
    Router::new()
        .route("/api/scenario/{id}/run", post(run_scenario))
        .route("/api/scenario-report/{report_id}", get(scenario_report))
        .with_state(RunState {
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
    #[serde(default)]
    environment_id: Option<String>,
    #[serde(default = "default_strategy")]
    failure_strategy: String,
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
    match st.reports.detail(&report_id).await {
        Ok(Some(d)) => (
            StatusCode::OK,
            Json(ScenarioReportResponse {
                report_id,
                status: d.status,
                case_count: d.case_count,
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
    status: String,
    case_count: usize,
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
                    Some((k.to_string(), it.get("value").and_then(|x| x.as_str()).unwrap_or("").to_string()))
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
                request: RequestSpec { method: method_of(&r.method), url, headers, body: r.body.clone() },
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

/// 静态叶子数:循环体只算一次(不乘 times)。
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
    let mut once = 0u32;
    let nodes = to_nodes(&plan, &mut once);
    let count = count_leaves(&nodes);
    if count == 0 {
        return (StatusCode::BAD_REQUEST, "scenario has no runnable steps").into_response();
    }
    let env = match req.environment_id.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(eid) => match st.envs.resolve(eid).await {
            Ok(e) => e.unwrap_or_default(),
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "env resolve error").into_response(),
        },
        None => Default::default(),
    };
    let stop_on_failure = req.failure_strategy.eq_ignore_ascii_case("STOP");

    let report_id = match st.reports.create("SERIAL", count as i32).await {
        Ok(r) => r,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "create report error").into_response(),
    };

    let all_pass = match st.executor.run(&report_id, &nodes, &env, stop_on_failure).await {
        Ok(p) => p,
        Err(_) => {
            let _ = st.reports.set_status(&report_id, "ERROR").await;
            return (StatusCode::INTERNAL_SERVER_ERROR, "execution error").into_response();
        }
    };
    let status = if all_pass { "SUCCESS" } else { "ERROR" };

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
