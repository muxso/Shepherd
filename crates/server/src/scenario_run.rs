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

    pub async fn run(
        &self,
        scenario_id: &str,
        project_id: &str,
        environment_id: Option<&str>,
        stop_on_failure: bool,
    ) -> Result<RunOutcome, RunError> {
        let (nodes, count) = self.prepare(scenario_id).await?;
        let env = self.resolve_env(environment_id).await?;
        let report_id = self
            .reports
            .create("SERIAL", count as i32)
            .await
            .map_err(|_| RunError::Internal("create report error"))?;
        let all_pass = match self.executor.run(&report_id, &nodes, &env, stop_on_failure).await {
            Ok(p) => p,
            Err(_) => {
                let _ = self.reports.set_status(&report_id, "ERROR").await;
                return Err(RunError::Internal("execution error"));
            }
        };
        let status = if all_pass { "SUCCESS" } else { "ERROR" };
        let _ = self.reports.set_status(&report_id, status).await;
        let _ = self
            .recorder
            .execute(scenario_id, project_id, status, count as i32, Some(&report_id))
            .await;
        Ok(RunOutcome { report_id, status: status.to_string(), case_count: count })
    }
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
            let env = match st.runner.resolve_env(env_id.as_deref()).await {
                Ok(e) => e,
                Err(_) => Default::default(),
            };
            let pass = st.runner.executor.run(&report_id, nodes, &env, false).await.unwrap_or(false);
            let status = if pass { "SUCCESS" } else { "ERROR" };
            if pass {
                success += 1;
            } else if b.stop_on_fail {
                stopped = true;
            }
            let _ = st
                .runner
                .recorder
                .execute(sid, &b.project_id, status, *count as i32, Some(&report_id))
                .await;
            results.push(BatchRunItem {
                scenario_id: sid.clone(),
                report_id: Some(report_id.clone()),
                status: status.to_string(),
            });
        }
        let overall = if success == prepared.len() { "SUCCESS" } else { "ERROR" };
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
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let env_id = effective_env(&runner, override_env.as_deref(), &sid).await;
                let out = runner.run(&sid, &project, env_id.as_deref(), false).await;
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
            match st.runner.run(sid, &b.project_id, env_id.as_deref(), false).await {
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
    match st.runner.run(&id, &req.project_id, req.environment_id.as_deref(), stop_on_failure).await
    {
        Ok(o) => (
            StatusCode::OK,
            Json(RunScenarioResponse {
                report_id: o.report_id,
                status: o.status,
                case_count: o.case_count,
            }),
        )
            .into_response(),
        Err(RunError::NotFound) => (StatusCode::NOT_FOUND, "scenario not found").into_response(),
        Err(RunError::CycleOrDepth) => {
            (StatusCode::CONFLICT, "scenario reference cycle or too deep").into_response()
        }
        Err(RunError::NoSteps) => {
            (StatusCode::BAD_REQUEST, "scenario has no runnable steps").into_response()
        }
        Err(RunError::Internal(msg)) => (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response(),
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
