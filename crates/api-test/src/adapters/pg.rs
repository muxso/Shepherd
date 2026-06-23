//! PostgreSQL 适配器:资源池信息源 + 批量执行器。
//!
//! - `PgResourcePool` 实现 `ResourcePoolPort`:查项目默认池 + 池可用性。
//! - `PgBatchReportExecutor` 实现 `BatchExecutorPort`:**落 PENDING 报告 → 经 `TaskDispatcher`
//!   下发执行节点 → 据结果置 RUNNING / DISPATCH_FAILED**。下发的真实传输由注入的
//!   `TaskDispatcher` 决定(生产用 `api-test-jmeter` 的 HTTP 下发,测试用 Spy)。

use std::sync::Arc;

use async_trait::async_trait;
use crate::domain::{NewResourcePool, ResolvedEnv, ResourcePool};
use crate::ports::{
    BatchExecutorPort, DispatchOutcome, DispatchReport, DispatchSpec, EnvironmentPort, PortError,
    ResourcePoolAdminPort, ResourcePoolPort, RunTask, TaskDispatcher,
};
use sqlx::{PgPool, Row};

fn map_err(e: sqlx::Error) -> PortError {
    PortError::Backend(e.to_string())
}

// ---- 资源池信息源 ----
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
        // 无配置行 → None;有行但列为 NULL → 也是 None
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

// ---- 资源池管理(创建 / 列出)----
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
        // id 走表默认 gen_random_uuid()::text(见迁移 0025)。
        let row = sqlx::query(
            "INSERT INTO ms_resource_pool (name, enabled, deleted) VALUES ($1, $2, false) \
             RETURNING id, name, enabled",
        )
        .bind(&new_pool.name)
        .bind(new_pool.enabled)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(ResourcePool {
            id: row.try_get("id").map_err(map_err)?,
            name: row.try_get("name").map_err(map_err)?,
            enabled: row.try_get("enabled").map_err(map_err)?,
        })
    }

    async fn list(&self) -> Result<Vec<ResourcePool>, PortError> {
        let rows = sqlx::query(
            "SELECT id, name, enabled FROM ms_resource_pool WHERE NOT deleted ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.into_iter()
            .map(|r| {
                Ok(ResourcePool {
                    id: r.try_get("id").map_err(map_err)?,
                    name: r.try_get("name").map_err(map_err)?,
                    enabled: r.try_get("enabled").map_err(map_err)?,
                })
            })
            .collect()
    }
}

// ---- 运行环境信息源 ----
// 直读 environment 上下文的 ms_environment 表(与 PgResourcePool 直读 ms_resource_pool 同构:
// 跨上下文只读邻域表,不引入 crate 依赖)。headers JSONB 数组、variables JSONB 对象。
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
    /// 把提取的「环境参数」合并进该环境的 variables(JSONB 对象 `||` 合并;同名覆盖)。
    async fn set_vars(&self, environment_id: &str, vars: &[(String, String)]) -> Result<(), PortError> {
        if vars.is_empty() {
            return Ok(());
        }
        let obj = serde_json::Value::Object(
            vars.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect(),
        );
        sqlx::query("UPDATE ms_environment SET variables = variables || $2 WHERE id = $1 AND NOT deleted")
            .bind(environment_id)
            .bind(&obj)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }
}

// ---- 批量执行器:落报告 + 下发执行节点 ----
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
        // 1) 落 PENDING 报告
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

        // 2) 下发执行节点(HTTP → JMeter)
        let task = RunTask {
            report_id: report_id.clone(),
            pool_id: spec.pool_id.clone(),
            mode: spec.mode,
            case_ids: spec.case_ids.clone(),
            env: spec.env.clone(),
            environment_id: spec.environment_id.clone(),
        };
        match self.dispatcher.dispatch_task(&task).await {
            // 3) 据结果更新报告状态
            Ok(DispatchOutcome::Accepted) => {
                // 异步执行器(JMeter):已接受,远端运行中
                self.set_status(&report_id, "RUNNING").await?;
                Ok(DispatchReport { report_id, status: "RUNNING".to_string() })
            }
            Ok(DispatchOutcome::Completed { status }) => {
                // 同步执行器(原生 runner):就地跑完,写最终状态
                self.set_status(&report_id, &status).await?;
                Ok(DispatchReport { report_id, status })
            }
            Err(e) => {
                // 下发失败:报告标记 DISPATCH_FAILED 并向上报错(不让任务"卡在 PENDING")
                let _ = self.set_status(&report_id, "DISPATCH_FAILED").await;
                Err(e)
            }
        }
    }
}

// ---- 用例规格源:供原生 runner 取 ms_api_case 的请求+断言 ----
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

/// JSONB `[{key,value,enabled?}]` → `(名, 值)` 列表;非数组/缺名/`enabled:false` 跳过。
fn parse_kv(v: &serde_json::Value) -> Vec<(String, String)> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|it| {
                    // 显式 enabled:false 视为停用,不参与执行(请求头/Query 共用)。
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

/// 认证对象 `{type,token}` → 一条 Authorization 头(none/空则 None)。
/// bearer → `Bearer <token>`;basic → `Basic <token>`(token 视为已编码或 user:pass,原样透传)。
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

/// 把 query 参数拼到 url 上(已带 `?` 则用 `&` 续接;不做 URL 编码,与既有调试链路一致)。
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
        // 处理器宽容解析:脏数据回落空,不阻断执行。
        let processors_json: serde_json::Value = r.try_get("processors").map_err(map_err)?;
        let processors: Vec<Processor> = serde_json::from_value(processors_json).unwrap_or_default();

        // 请求头 = 存储的 headers + 认证头;query 合入 URL(宽容解析,脏数据不阻断)。
        let headers_json: serde_json::Value = r.try_get("headers").unwrap_or_else(|_| serde_json::json!([]));
        let query_json: serde_json::Value = r.try_get("query_params").unwrap_or_else(|_| serde_json::json!([]));
        let auth_json: serde_json::Value = r.try_get("auth").unwrap_or_else(|_| serde_json::json!({}));
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

// ---- 结果汇:把 per-case 明细 UPSERT 进 ms_api_case_result ----
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
    ) -> Result<(), PortError> {
        // 响应体截断到 64KB,避免明细表膨胀。
        let body_trunc: String = body.chars().take(65536).collect();
        let headers_json = serde_json::to_value(headers)
            .map_err(|e| PortError::Backend(format!("serialize headers: {e}")))?;
        sqlx::query(
            "UPDATE ms_api_case_result \
               SET status_code = $3, latency_ms = $4, resp_size = $5, body = $6, headers = $7, \
                   assertions = $8, extractions = $9 \
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
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }
}

// ---- 批量报告读写:供计划树执行(场景)外层补建报告 + 回写最终状态 ----
// 计划树执行不经资源池(本地 runner 就地跑),pool_id 落占位 'local'。
#[derive(Clone)]
pub struct PgBatchReport {
    pool: PgPool,
}

impl PgBatchReport {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 落一行 RUNNING 报告,返回 report_id(用例结果按此 report_id 归组)。
    pub async fn create(&self, run_mode: &str, case_count: i32) -> Result<String, PortError> {
        let row = sqlx::query(
            "INSERT INTO ms_api_batch_report (pool_id, run_mode, case_count, status) \
             VALUES ('local', $1, $2, 'RUNNING') RETURNING id",
        )
        .bind(run_mode)
        .bind(case_count)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row.try_get("id").map_err(map_err)
    }

    /// 回写报告最终状态(SUCCESS/ERROR)。
    pub async fn set_status(&self, report_id: &str, status: &str) -> Result<(), PortError> {
        sqlx::query("UPDATE ms_api_batch_report SET status = $2 WHERE id = $1")
            .bind(report_id)
            .bind(status)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    /// 报告明细:报告头(状态/用例数)+ 逐用例结果(case_id/通过失败/失败原因/时间)。
    /// 不存在返回 None。注:当前未持久化响应时间/状态码/响应体(需执行器扩展)。
    pub async fn detail(&self, report_id: &str) -> Result<Option<BatchReportDetail>, PortError> {
        let header = sqlx::query("SELECT status, case_count FROM ms_api_batch_report WHERE id = $1")
            .bind(report_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?;
        let Some(h) = header else { return Ok(None) };
        let rows = sqlx::query(
            "SELECT case_id, outcome, failures, executed_at::text AS executed_at, \
                    status_code, latency_ms, resp_size, body, headers, assertions, extractions \
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
                // 断言/提取(0048 后回填;旧报告为 null → 空数组)。
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
                })
            })
            .collect::<Result<Vec<_>, PortError>>()?;
        Ok(Some(BatchReportDetail {
            status: h.try_get("status").map_err(map_err)?,
            case_count: h.try_get("case_count").map_err(map_err)?,
            results,
        }))
    }
}

/// 报告明细读模型(报告头 + 逐用例结果)。供场景报告页渲染。
#[derive(Debug, Clone)]
pub struct BatchReportDetail {
    pub status: String,
    pub case_count: i32,
    pub results: Vec<CaseResultRow>,
}

/// 单条用例结果(report 明细行)。
#[derive(Debug, Clone)]
pub struct CaseResultRow {
    pub case_id: String,
    pub outcome: String,
    pub failures: Vec<String>,
    pub executed_at: String,
    /// 响应明细(0045 后回填;旧行为 None)。
    pub status_code: Option<i32>,
    pub latency_ms: Option<i64>,
    pub resp_size: Option<i64>,
    pub body: Option<String>,
    pub headers: Vec<(String, String)>,
    /// 逐条断言结果 / 提取变量(0048 后回填;旧报告为空数组)。
    pub assertions: serde_json::Value,
    pub extractions: serde_json::Value,
}

// ---- 用例执行记录读模型:按 case_id 倒序分页查 ms_api_case_result ----
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
        // sqlx 未启用 chrono/time feature,故在 SQL 内用 to_char 把 TIMESTAMPTZ 归一为 RFC3339 字符串。
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
    use crate::adapters::{SpyDispatcher, NoopDispatcher};
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
        assert_eq!(merge_query("/x?p=0", &[("a".into(), "1".into()), ("b".into(), "2".into())]), "/x?p=0&a=1&b=2");
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
        assert_eq!(rp.default_pool_id("proj2").await.expect("d"), None); // 列为 NULL
        assert_eq!(rp.default_pool_id("ghost").await.expect("d"), None); // 无配置行
        assert!(rp.is_pool_available("pool1").await.expect("a"));
        assert!(!rp.is_pool_available("pool2").await.expect("a")); // 禁用
        assert!(!rp.is_pool_available("nope").await.expect("a")); // 不存在

        let spec = DispatchSpec {
            case_ids: vec!["c1".into(), "c2".into()],
            pool_id: "pool1".into(),
            mode: BatchRunMode::Parallel,
            env: ResolvedEnv::default(),
        };

        // 下发成功:报告 RUNNING,且下发器收到带 report_id 的任务
        let spy = SpyDispatcher::new();
        let exec = PgBatchReportExecutor::new(pool.clone(), Arc::new(spy.clone()));
        let rep = exec.dispatch(&spec).await.expect("dispatch");
        assert_eq!(rep.status, "RUNNING"); // 回传状态与报告一致
        let row = sqlx::query("SELECT case_count, status FROM ms_api_batch_report WHERE id = $1")
            .bind(&rep.report_id)
            .fetch_one(&pool)
            .await
            .expect("report row");
        assert_eq!(row.try_get::<i32, _>("case_count").expect("cc"), 2);
        assert_eq!(row.try_get::<String, _>("status").expect("st"), "RUNNING");
        let task = spy.last().expect("dispatched");
        assert_eq!(task.report_id, rep.report_id); // report_id 透传给执行节点
        assert_eq!(task.case_ids.len(), 2);

        // 下发失败:报告 DISPATCH_FAILED,且 dispatch 向上报错
        let exec_fail = PgBatchReportExecutor::new(pool.clone(), Arc::new(SpyDispatcher::failing()));
        let err = exec_fail.dispatch(&spec).await;
        assert!(err.is_err());
        let failed = sqlx::query(
            "SELECT status FROM ms_api_batch_report WHERE status = 'DISPATCH_FAILED'",
        )
        .fetch_optional(&pool)
        .await
        .expect("q");
        assert!(failed.is_some());

        // NoopDispatcher 也能用(本地无执行节点)
        let _ = PgBatchReportExecutor::new(pool.clone(), Arc::new(NoopDispatcher));
    }

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_case_execution_query_pagination() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::raw_sql("TRUNCATE ms_api_case_result").execute(&pool).await.expect("truncate");
        // 三条记录,executed_at 递增 → 倒序后 r3、r2、r1
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

        // 第 1 页 2 条:倒序 → r3, r2
        let page1 = q.list_by_case("c1", 0, 2).await.expect("page1");
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].report_id, "r3");
        assert_eq!(page1[1].report_id, "r2");
        assert_eq!(page1[0].outcome, "SUCCESS");
        assert_eq!(page1[1].failures, serde_json::json!(["boom"]));
        assert_eq!(page1[0].executed_at, "2026-05-31T00:00:03Z"); // RFC3339 归一

        // 第 2 页(offset=2)剩 r1
        let page2 = q.list_by_case("c1", 2, 2).await.expect("page2");
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].report_id, "r1");
    }
}
