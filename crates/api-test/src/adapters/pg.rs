use std::sync::Arc;

use crate::domain::{NewResourcePool, ResolvedEnv, ResourcePool};
use crate::ports::{
    BatchExecutorPort, DispatchOutcome, DispatchReport, DispatchSpec, EnvironmentPort, PortError,
    ResourcePoolAdminPort, ResourcePoolPort, RunTask, TaskDispatcher,
};
use async_trait::async_trait;
use sqlx::{PgPool, Row};

fn map_err(e: sqlx::Error) -> PortError {
    PortError::Backend(e.to_string())
}

#[derive(Clone)]
pub struct PgResourcePool {
    pool: PgPool,
}

impl PgResourcePool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ResourcePoolPort for PgResourcePool {
    async fn default_pool_id(&self, project_id: &str) -> Result<Option<String>, PortError> {
        let row =
            sqlx::query("SELECT default_pool_id FROM ms_project_api_config WHERE project_id = $1")
                .bind(project_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_err)?;
        match row {
            Some(r) => Ok(r.try_get::<Option<String>, _>("default_pool_id").map_err(map_err)?),
            None => Ok(None),
        }
    }

    async fn is_pool_available(&self, pool_id: &str) -> Result<bool, PortError> {
        let row = sqlx::query(
            "SELECT EXISTS(SELECT 1 FROM ms_resource_pool \
             WHERE id = $1 AND enabled AND NOT deleted) AS ok",
        )
        .bind(pool_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(row.try_get::<bool, _>("ok").map_err(map_err)?)
    }
}

const POOL_COLS: &str = "id, name, enabled, description, max_concurrency, pool_type, all_org, org_ids, server_url, config, \
     to_char(created_at, 'YYYY-MM-DD HH24:MI:SS') AS created_at, \
     to_char(updated_at, 'YYYY-MM-DD HH24:MI:SS') AS updated_at";

fn map_pool(r: &sqlx::postgres::PgRow) -> Result<ResourcePool, PortError> {
    Ok(ResourcePool {
        id: r.try_get("id").map_err(map_err)?,
        name: r.try_get("name").map_err(map_err)?,
        enabled: r.try_get("enabled").map_err(map_err)?,
        description: r.try_get("description").map_err(map_err)?,
        max_concurrency: r.try_get("max_concurrency").map_err(map_err)?,
        pool_type: r.try_get("pool_type").map_err(map_err)?,
        all_org: r.try_get("all_org").map_err(map_err)?,
        org_ids: r.try_get::<sqlx::types::Json<Vec<String>>, _>("org_ids").map_err(map_err)?.0,
        server_url: r.try_get("server_url").map_err(map_err)?,
        config: r.try_get::<sqlx::types::Json<serde_json::Value>, _>("config").map_err(map_err)?.0,
        created_at: r.try_get("created_at").map_err(map_err)?,
        updated_at: r.try_get("updated_at").map_err(map_err)?,
    })
}

#[derive(Clone)]
pub struct PgResourcePoolAdmin {
    pool: PgPool,
}

impl PgResourcePoolAdmin {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ResourcePoolAdminPort for PgResourcePoolAdmin {
    async fn create(&self, new_pool: &NewResourcePool) -> Result<ResourcePool, PortError> {
        // id comes from the table default gen_random_uuid()::text, so it is not bound.
        let sql = format!(
            "INSERT INTO ms_resource_pool \
             (name, enabled, description, max_concurrency, pool_type, all_org, org_ids, server_url, config, deleted) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, false) RETURNING {POOL_COLS}"
        );
        let row = sqlx::query(&sql)
            .bind(&new_pool.name)
            .bind(new_pool.enabled)
            .bind(&new_pool.description)
            .bind(new_pool.max_concurrency)
            .bind(&new_pool.pool_type)
            .bind(new_pool.all_org)
            .bind(sqlx::types::Json(&new_pool.org_ids))
            .bind(&new_pool.server_url)
            .bind(sqlx::types::Json(&new_pool.config))
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)?;
        map_pool(&row)
    }

    async fn list(&self) -> Result<Vec<ResourcePool>, PortError> {
        let sql = format!(
            "SELECT {POOL_COLS} FROM ms_resource_pool WHERE NOT deleted ORDER BY created_at DESC"
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await.map_err(map_err)?;
        rows.iter().map(map_pool).collect()
    }

    async fn get(&self, id: &str) -> Result<Option<ResourcePool>, PortError> {
        let sql = format!("SELECT {POOL_COLS} FROM ms_resource_pool WHERE id = $1 AND NOT deleted");
        let row = sqlx::query(&sql).bind(id).fetch_optional(&self.pool).await.map_err(map_err)?;
        row.as_ref().map(map_pool).transpose()
    }

    async fn update(
        &self,
        id: &str,
        new_pool: &NewResourcePool,
    ) -> Result<Option<ResourcePool>, PortError> {
        let sql = format!(
            "UPDATE ms_resource_pool SET \
             name = $2, enabled = $3, description = $4, max_concurrency = $5, pool_type = $6, \
             all_org = $7, org_ids = $8, server_url = $9, config = $10, updated_at = now() \
             WHERE id = $1 AND NOT deleted RETURNING {POOL_COLS}"
        );
        let row = sqlx::query(&sql)
            .bind(id)
            .bind(&new_pool.name)
            .bind(new_pool.enabled)
            .bind(&new_pool.description)
            .bind(new_pool.max_concurrency)
            .bind(&new_pool.pool_type)
            .bind(new_pool.all_org)
            .bind(sqlx::types::Json(&new_pool.org_ids))
            .bind(&new_pool.server_url)
            .bind(sqlx::types::Json(&new_pool.config))
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?;
        row.as_ref().map(map_pool).transpose()
    }

    async fn delete(&self, id: &str) -> Result<bool, PortError> {
        let res = sqlx::query(
            "UPDATE ms_resource_pool SET deleted = true, updated_at = now() WHERE id = $1 AND NOT deleted",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }
}

#[derive(Clone)]
pub struct PgEnvironment {
    pool: PgPool,
}

impl PgEnvironment {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EnvironmentPort for PgEnvironment {
    async fn resolve(&self, environment_id: &str) -> Result<Option<ResolvedEnv>, PortError> {
        let row = sqlx::query(
            "SELECT base_url, headers, variables FROM ms_environment \
             WHERE id = $1 AND enabled AND NOT deleted",
        )
        .bind(environment_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        let Some(row) = row else { return Ok(None) };
        let base_url: String = row.try_get("base_url").map_err(map_err)?;
        let headers_json: serde_json::Value = row.try_get("headers").map_err(map_err)?;
        let vars_json: serde_json::Value = row.try_get("variables").map_err(map_err)?;
        let headers = headers_json
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|h| {
                        let name = h.get("name")?.as_str()?.to_string();
                        let value =
                            h.get("value").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        Some((name, value))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let variables = vars_json
            .as_object()
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| {
                        let s = match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        (k.clone(), s)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Some(ResolvedEnv { base_url, headers, variables }))
    }
}

#[async_trait]
impl crate::ports::EnvVarWriter for PgEnvironment {
    async fn set_vars(
        &self,
        environment_id: &str,
        vars: &[(String, String)],
    ) -> Result<(), PortError> {
        if vars.is_empty() {
            return Ok(());
        }
        let obj = serde_json::Value::Object(
            vars.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect(),
        );
        sqlx::query(
            "UPDATE ms_environment SET variables = variables || $2 WHERE id = $1 AND NOT deleted",
        )
        .bind(environment_id)
        .bind(&obj)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct PgBatchReportExecutor {
    pool: PgPool,
    dispatcher: Arc<dyn TaskDispatcher>,
}

impl PgBatchReportExecutor {
    pub fn new(pool: PgPool, dispatcher: Arc<dyn TaskDispatcher>) -> Self {
        Self { pool, dispatcher }
    }

    async fn set_status(&self, report_id: &str, status: &str) -> Result<(), PortError> {
        sqlx::query("UPDATE ms_api_batch_report SET status = $2 WHERE id = $1")
            .bind(report_id)
            .bind(status)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }
}

#[async_trait]
impl BatchExecutorPort for PgBatchReportExecutor {
    async fn dispatch(&self, spec: &DispatchSpec) -> Result<DispatchReport, PortError> {
        let row = sqlx::query(
            "INSERT INTO ms_api_batch_report (pool_id, run_mode, case_count) \
             VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(&spec.pool_id)
        .bind(spec.mode.as_str())
        .bind(spec.case_ids.len() as i32)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        let report_id: String = row.try_get("id").map_err(map_err)?;

        let task = RunTask {
            report_id: report_id.clone(),
            pool_id: spec.pool_id.clone(),
            mode: spec.mode,
            case_ids: spec.case_ids.clone(),
            env: spec.env.clone(),
            environment_id: spec.environment_id.clone(),
        };
        match self.dispatcher.dispatch_task(&task).await {
            Ok(DispatchOutcome::Accepted) => {
                self.set_status(&report_id, "RUNNING").await?;
                Ok(DispatchReport { report_id, status: "RUNNING".to_string() })
            }
            Ok(DispatchOutcome::Completed { status }) => {
                self.set_status(&report_id, &status).await?;
                Ok(DispatchReport { report_id, status })
            }
            Err(e) => {
                // Mark DISPATCH_FAILED so the task doesn't stay stuck in PENDING.
                let _ = self.set_status(&report_id, "DISPATCH_FAILED").await;
                Err(e)
            }
        }
    }
}

use crate::adapters::local::{CaseResultSink, CaseRunSpec, CaseSpecSource};
use api_runner::{Assertion, HttpMethod, Processor, RequestSpec};

#[derive(Clone)]
pub struct PgCaseSpecSource {
    pool: PgPool,
}

impl PgCaseSpecSource {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn parse_method(s: &str) -> HttpMethod {
    match s.trim().to_uppercase().as_str() {
        "POST" => HttpMethod::Post,
        "PUT" => HttpMethod::Put,
        "DELETE" => HttpMethod::Delete,
        "PATCH" => HttpMethod::Patch,
        _ => HttpMethod::Get,
    }
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
    let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("none");
    let token = v.get("token").and_then(|x| x.as_str()).unwrap_or("").trim();
    if token.is_empty() {
        return None;
    }
    match ty {
        "bearer" => Some(("Authorization".to_string(), format!("Bearer {token}"))),
        "basic" => Some(("Authorization".to_string(), format!("Basic {token}"))),
        _ => None,
    }
}

// No URL encoding, matching the existing debug path.
fn merge_query(url: &str, query: &[(String, String)]) -> String {
    if query.is_empty() {
        return url.to_string();
    }
    let qs = query.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&");
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}{qs}")
}

#[async_trait]
impl CaseSpecSource for PgCaseSpecSource {
    async fn spec_of(&self, case_id: &str) -> Result<Option<CaseRunSpec>, PortError> {
        let row = sqlx::query(
            "SELECT method, url, body, assertions, processors, headers, query_params, auth \
             FROM ms_api_case WHERE id = $1",
        )
        .bind(case_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        let Some(r) = row else { return Ok(None) };

        let method: String = r.try_get("method").map_err(map_err)?;
        let url: String = r.try_get("url").map_err(map_err)?;
        let body: Option<String> = r.try_get("body").map_err(map_err)?;
        let assertions_json: serde_json::Value = r.try_get("assertions").map_err(map_err)?;
        let assertions: Vec<Assertion> = serde_json::from_value(assertions_json)
            .map_err(|e| PortError::Backend(format!("bad assertions json: {e}")))?;
        let processors_json: serde_json::Value = r.try_get("processors").map_err(map_err)?;
        let processors: Vec<Processor> =
            serde_json::from_value(processors_json).unwrap_or_default();

        let headers_json: serde_json::Value =
            r.try_get("headers").unwrap_or_else(|_| serde_json::json!([]));
        let query_json: serde_json::Value =
            r.try_get("query_params").unwrap_or_else(|_| serde_json::json!([]));
        let auth_json: serde_json::Value =
            r.try_get("auth").unwrap_or_else(|_| serde_json::json!({}));
        let mut headers = parse_kv(&headers_json);
        if let Some(a) = auth_header(&auth_json) {
            headers.push(a);
        }
        let url = merge_query(&url, &parse_kv(&query_json));

        Ok(Some(CaseRunSpec {
            request: RequestSpec { method: parse_method(&method), url, headers, body },
            assertions,
            processors,
        }))
    }
}

#[derive(Clone)]
pub struct PgCaseResultSink {
    pool: PgPool,
}

impl PgCaseResultSink {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CaseResultSink for PgCaseResultSink {
    async fn record(
        &self,
        report_id: &str,
        case_id: &str,
        outcome: &str,
        failures: &[String],
    ) -> Result<(), PortError> {
        let failures_json = serde_json::to_value(failures)
            .map_err(|e| PortError::Backend(format!("serialize failures: {e}")))?;
        sqlx::query(
            "INSERT INTO ms_api_case_result (report_id, case_id, outcome, failures) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (report_id, case_id) DO UPDATE \
               SET outcome = EXCLUDED.outcome, failures = EXCLUDED.failures",
        )
        .bind(report_id)
        .bind(case_id)
        .bind(outcome)
        .bind(failures_json)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_detail(
        &self,
        report_id: &str,
        case_id: &str,
        status_code: i32,
        latency_ms: i64,
        resp_size: i64,
        body: &str,
        headers: &[(String, String)],
        assertions: &serde_json::Value,
        extractions: &serde_json::Value,
        req_method: &str,
        req_url: &str,
        req_headers: &[(String, String)],
        req_body: Option<&str>,
    ) -> Result<(), PortError> {
        // Truncate to 64KB to keep the detail table from bloating.
        let body_trunc: String = body.chars().take(65536).collect();
        let req_body_trunc: Option<String> = req_body.map(|b| b.chars().take(65536).collect());
        let headers_json = serde_json::to_value(headers)
            .map_err(|e| PortError::Backend(format!("serialize headers: {e}")))?;
        let req_headers_json = serde_json::to_value(req_headers)
            .map_err(|e| PortError::Backend(format!("serialize req headers: {e}")))?;
        sqlx::query(
            "UPDATE ms_api_case_result \
               SET status_code = $3, latency_ms = $4, resp_size = $5, body = $6, headers = $7, \
                   assertions = $8, extractions = $9, \
                   req_method = $10, req_url = $11, req_headers = $12, req_body = $13 \
             WHERE report_id = $1 AND case_id = $2",
        )
        .bind(report_id)
        .bind(case_id)
        .bind(status_code)
        .bind(latency_ms)
        .bind(resp_size)
        .bind(body_trunc)
        .bind(headers_json)
        .bind(assertions)
        .bind(extractions)
        .bind(req_method)
        .bind(req_url)
        .bind(req_headers_json)
        .bind(req_body_trunc)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }
}

// Plan-tree execution bypasses resource pools (the local runner runs in place); pool_id gets the placeholder 'local'.
#[derive(Clone)]
pub struct PgBatchReport {
    pool: PgPool,
}

impl PgBatchReport {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, run_mode: &str, case_count: i32) -> Result<String, PortError> {
        let row = sqlx::query(
            "INSERT INTO ms_api_batch_report (pool_id, run_mode, case_count, status, started_at) \
             VALUES ('local', $1, $2, 'RUNNING', now()) RETURNING id",
        )
        .bind(run_mode)
        .bind(case_count)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row.try_get("id").map_err(map_err)
    }

    pub async fn set_status(&self, report_id: &str, status: &str) -> Result<(), PortError> {
        sqlx::query(
            "UPDATE ms_api_batch_report SET status = $2, finished_at = now() WHERE id = $1",
        )
        .bind(report_id)
        .bind(status)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    pub async fn list_unarchived(&self, limit: i64) -> Result<Vec<String>, PortError> {
        let rows = sqlx::query(
            "SELECT id FROM ms_api_batch_report \
             WHERE status IN ('SUCCESS', 'ERROR') AND archived_at IS NULL \
             ORDER BY id LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(|r| r.try_get::<String, _>("id").map_err(map_err)).collect()
    }

    pub async fn mark_archived(&self, report_id: &str) -> Result<(), PortError> {
        sqlx::query("UPDATE ms_api_batch_report SET archived_at = now() WHERE id = $1")
            .bind(report_id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    pub async fn detail(&self, report_id: &str) -> Result<Option<BatchReportDetail>, PortError> {
        let header = sqlx::query(
            "SELECT status, case_count, started_at::text AS started_at, finished_at::text AS finished_at, \
                    (EXTRACT(EPOCH FROM (finished_at - started_at)) * 1000)::bigint AS duration_ms \
             FROM ms_api_batch_report WHERE id = $1",
        )
        .bind(report_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        let Some(h) = header else { return Ok(None) };
        let rows = sqlx::query(
            "SELECT case_id, outcome, failures, executed_at::text AS executed_at, \
                    status_code, latency_ms, resp_size, body, headers, assertions, extractions, \
                    req_method, req_url, req_headers, req_body \
             FROM ms_api_case_result WHERE report_id = $1 ORDER BY executed_at",
        )
        .bind(report_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let results = rows
            .iter()
            .map(|r| {
                let failures_v: serde_json::Value =
                    r.try_get("failures").unwrap_or_else(|_| serde_json::Value::Array(vec![]));
                let failures = failures_v
                    .as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let headers_v: Option<serde_json::Value> = r.try_get("headers").ok().flatten();
                let headers = headers_v
                    .and_then(|v| serde_json::from_value::<Vec<(String, String)>>(v).ok())
                    .unwrap_or_default();
                let req_headers_v: Option<serde_json::Value> =
                    r.try_get("req_headers").ok().flatten();
                let req_headers = req_headers_v
                    .and_then(|v| serde_json::from_value::<Vec<(String, String)>>(v).ok())
                    .unwrap_or_default();
                let json_arr = |col: &str| -> serde_json::Value {
                    r.try_get::<Option<serde_json::Value>, _>(col)
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| serde_json::Value::Array(vec![]))
                };
                Ok(CaseResultRow {
                    case_id: r.try_get("case_id").map_err(map_err)?,
                    outcome: r.try_get("outcome").map_err(map_err)?,
                    failures,
                    executed_at: r.try_get::<String, _>("executed_at").map_err(map_err)?,
                    status_code: r.try_get("status_code").ok().flatten(),
                    latency_ms: r.try_get("latency_ms").ok().flatten(),
                    resp_size: r.try_get("resp_size").ok().flatten(),
                    body: r.try_get("body").ok().flatten(),
                    headers,
                    assertions: json_arr("assertions"),
                    extractions: json_arr("extractions"),
                    req_method: r.try_get("req_method").ok().flatten(),
                    req_url: r.try_get("req_url").ok().flatten(),
                    req_headers,
                    req_body: r.try_get("req_body").ok().flatten(),
                })
            })
            .collect::<Result<Vec<_>, PortError>>()?;
        Ok(Some(BatchReportDetail {
            status: h.try_get("status").map_err(map_err)?,
            case_count: h.try_get("case_count").map_err(map_err)?,
            started_at: h.try_get("started_at").ok().flatten(),
            finished_at: h.try_get("finished_at").ok().flatten(),
            duration_ms: h.try_get("duration_ms").ok().flatten(),
            results,
        }))
    }
}

#[derive(Debug, Clone)]
pub struct BatchReportDetail {
    pub status: String,
    pub case_count: i32,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub results: Vec<CaseResultRow>,
}

#[derive(Debug, Clone)]
pub struct CaseResultRow {
    pub case_id: String,
    pub outcome: String,
    pub failures: Vec<String>,
    pub executed_at: String,
    pub status_code: Option<i32>,
    pub latency_ms: Option<i64>,
    pub resp_size: Option<i64>,
    pub body: Option<String>,
    pub headers: Vec<(String, String)>,
    pub assertions: serde_json::Value,
    pub extractions: serde_json::Value,
    pub req_method: Option<String>,
    pub req_url: Option<String>,
    pub req_headers: Vec<(String, String)>,
    pub req_body: Option<String>,
}

use crate::ports::{CaseExecutionQueryPort, CaseExecutionRecord};

#[derive(Clone)]
pub struct PgCaseExecutionQuery {
    pool: PgPool,
}

impl PgCaseExecutionQuery {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CaseExecutionQueryPort for PgCaseExecutionQuery {
    async fn count_by_case(&self, case_id: &str) -> Result<u64, PortError> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM ms_api_case_result WHERE case_id = $1")
            .bind(case_id)
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)?;
        let n: i64 = row.try_get("n").map_err(map_err)?;
        Ok(n as u64)
    }

    async fn list_by_case(
        &self,
        case_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<CaseExecutionRecord>, PortError> {
        // sqlx has no chrono/time feature enabled, so normalize TIMESTAMPTZ to RFC3339 via to_char in SQL.
        let rows = sqlx::query(
            "SELECT report_id, case_id, outcome, failures, \
                    to_char(executed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS executed_at \
             FROM ms_api_case_result \
             WHERE case_id = $1 \
             ORDER BY executed_at DESC \
             LIMIT $2 OFFSET $3",
        )
        .bind(case_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(CaseExecutionRecord {
                    report_id: r.try_get("report_id").map_err(map_err)?,
                    case_id: r.try_get("case_id").map_err(map_err)?,
                    outcome: r.try_get("outcome").map_err(map_err)?,
                    failures: r.try_get("failures").map_err(map_err)?,
                    executed_at: r.try_get("executed_at").map_err(map_err)?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{NoopDispatcher, SpyDispatcher};
    use crate::domain::BatchRunMode;

    #[test]
    fn parse_kv_skips_blank_disabled_and_reads_values() {
        let v = serde_json::json!([
            {"key": "X-A", "value": "1"},
            {"key": " ", "value": "skip"},
            {"key": "X-B"},
            {"key": "X-Off", "value": "x", "enabled": false}
        ]);
        assert_eq!(parse_kv(&v), vec![("X-A".into(), "1".into()), ("X-B".into(), "".into())]);
        assert!(parse_kv(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn auth_header_bearer_and_basic_and_none() {
        assert_eq!(
            auth_header(&serde_json::json!({"type": "bearer", "token": "t"})),
            Some(("Authorization".into(), "Bearer t".into()))
        );
        assert_eq!(
            auth_header(&serde_json::json!({"type": "basic", "token": "dXNlcjpwYXNz"})),
            Some(("Authorization".into(), "Basic dXNlcjpwYXNz".into()))
        );
        assert_eq!(auth_header(&serde_json::json!({"type": "none"})), None);
        assert_eq!(auth_header(&serde_json::json!({"type": "bearer", "token": "  "})), None);
    }

    #[test]
    fn merge_query_appends_with_right_separator() {
        assert_eq!(merge_query("/x", &[("a".into(), "1".into())]), "/x?a=1");
        assert_eq!(
            merge_query("/x?p=0", &[("a".into(), "1".into()), ("b".into(), "2".into())]),
            "/x?p=0&a=1&b=2"
        );
        assert_eq!(merge_query("/x", &[]), "/x");
    }

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_pool_resolution_and_dispatch() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::raw_sql("TRUNCATE ms_resource_pool, ms_project_api_config, ms_api_batch_report")
            .execute(&pool)
            .await
            .expect("truncate");
        sqlx::raw_sql(
            "INSERT INTO ms_resource_pool (id,name,enabled,deleted) VALUES \
                ('pool1','可用',true,false),('pool2','禁用',false,false); \
             INSERT INTO ms_project_api_config (project_id, default_pool_id) VALUES \
                ('proj1','pool1'),('proj2',NULL);",
        )
        .execute(&pool)
        .await
        .expect("seed");

        let rp = PgResourcePool::new(pool.clone());
        assert_eq!(rp.default_pool_id("proj1").await.expect("d"), Some("pool1".into()));
        assert_eq!(rp.default_pool_id("proj2").await.expect("d"), None);
        assert_eq!(rp.default_pool_id("ghost").await.expect("d"), None);
        assert!(rp.is_pool_available("pool1").await.expect("a"));
        assert!(!rp.is_pool_available("pool2").await.expect("a"));
        assert!(!rp.is_pool_available("nope").await.expect("a"));

        let spec = DispatchSpec {
            case_ids: vec!["c1".into(), "c2".into()],
            pool_id: "pool1".into(),
            mode: BatchRunMode::Parallel,
            env: ResolvedEnv::default(),
            environment_id: None,
        };

        let spy = SpyDispatcher::new();
        let exec = PgBatchReportExecutor::new(pool.clone(), Arc::new(spy.clone()));
        let rep = exec.dispatch(&spec).await.expect("dispatch");
        assert_eq!(rep.status, "RUNNING");
        let row = sqlx::query("SELECT case_count, status FROM ms_api_batch_report WHERE id = $1")
            .bind(&rep.report_id)
            .fetch_one(&pool)
            .await
            .expect("report row");
        assert_eq!(row.try_get::<i32, _>("case_count").expect("cc"), 2);
        assert_eq!(row.try_get::<String, _>("status").expect("st"), "RUNNING");
        let task = spy.last().expect("dispatched");
        assert_eq!(task.report_id, rep.report_id);
        assert_eq!(task.case_ids.len(), 2);

        let exec_fail =
            PgBatchReportExecutor::new(pool.clone(), Arc::new(SpyDispatcher::failing()));
        let err = exec_fail.dispatch(&spec).await;
        assert!(err.is_err());
        let failed =
            sqlx::query("SELECT status FROM ms_api_batch_report WHERE status = 'DISPATCH_FAILED'")
                .fetch_optional(&pool)
                .await
                .expect("q");
        assert!(failed.is_some());

        let _ = PgBatchReportExecutor::new(pool.clone(), Arc::new(NoopDispatcher));
    }

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_case_execution_query_pagination() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::raw_sql("TRUNCATE ms_api_case_result").execute(&pool).await.expect("truncate");
        sqlx::raw_sql(
            "INSERT INTO ms_api_case_result (report_id, case_id, outcome, failures, executed_at) VALUES \
                ('r1','c1','SUCCESS','[]'::jsonb,        '2026-05-31T00:00:01Z'), \
                ('r2','c1','FAILED','[\"boom\"]'::jsonb, '2026-05-31T00:00:02Z'), \
                ('r3','c1','SUCCESS','[]'::jsonb,        '2026-05-31T00:00:03Z'), \
                ('r9','c2','SUCCESS','[]'::jsonb,        '2026-05-31T00:00:09Z');",
        )
        .execute(&pool)
        .await
        .expect("seed");

        let q = PgCaseExecutionQuery::new(pool.clone());
        assert_eq!(q.count_by_case("c1").await.expect("count"), 3);
        assert_eq!(q.count_by_case("c2").await.expect("count"), 1);

        let page1 = q.list_by_case("c1", 0, 2).await.expect("page1");
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].report_id, "r3");
        assert_eq!(page1[1].report_id, "r2");
        assert_eq!(page1[0].outcome, "SUCCESS");
        assert_eq!(page1[1].failures, serde_json::json!(["boom"]));
        assert_eq!(page1[0].executed_at, "2026-05-31T00:00:03Z");

        let page2 = q.list_by_case("c1", 2, 2).await.expect("page2");
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].report_id, "r1");
    }
}
