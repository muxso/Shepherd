//! PostgreSQL 适配器:资源池信息源 + 批量执行器。
//!
//! - `PgResourcePool` 实现 `ResourcePoolPort`:查项目默认池 + 池可用性。
//! - `PgBatchReportExecutor` 实现 `BatchExecutorPort`:**落 PENDING 报告 → 经 `TaskDispatcher`
//!   下发执行节点 → 据结果置 RUNNING / DISPATCH_FAILED**。下发的真实传输由注入的
//!   `TaskDispatcher` 决定(生产用 `api-test-jmeter` 的 HTTP 下发,测试用 Spy)。

use std::sync::Arc;

use async_trait::async_trait;
use crate::ports::{
    BatchExecutorPort, DispatchOutcome, DispatchSpec, PortError, ResourcePoolPort, RunTask,
    TaskDispatcher,
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
    async fn dispatch(&self, spec: &DispatchSpec) -> Result<String, PortError> {
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
        };
        match self.dispatcher.dispatch_task(&task).await {
            // 3) 据结果更新报告状态
            Ok(DispatchOutcome::Accepted) => {
                // 异步执行器(JMeter):已接受,远端运行中
                self.set_status(&report_id, "RUNNING").await?;
                Ok(report_id)
            }
            Ok(DispatchOutcome::Completed { status }) => {
                // 同步执行器(原生 runner):就地跑完,写最终状态
                self.set_status(&report_id, &status).await?;
                Ok(report_id)
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
use api_runner::{Assertion, HttpMethod, RequestSpec};

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

#[async_trait]
impl CaseSpecSource for PgCaseSpecSource {
    async fn spec_of(&self, case_id: &str) -> Result<Option<CaseRunSpec>, PortError> {
        let row = sqlx::query("SELECT method, url, body, assertions FROM ms_api_case WHERE id = $1")
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

        Ok(Some(CaseRunSpec {
            request: RequestSpec { method: parse_method(&method), url, headers: vec![], body },
            assertions,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{SpyDispatcher, NoopDispatcher};
    use crate::domain::BatchRunMode;

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
        };

        // 下发成功:报告 RUNNING,且下发器收到带 report_id 的任务
        let spy = SpyDispatcher::new();
        let exec = PgBatchReportExecutor::new(pool.clone(), Arc::new(spy.clone()));
        let report_id = exec.dispatch(&spec).await.expect("dispatch");
        let row = sqlx::query("SELECT case_count, status FROM ms_api_batch_report WHERE id = $1")
            .bind(&report_id)
            .fetch_one(&pool)
            .await
            .expect("report row");
        assert_eq!(row.try_get::<i32, _>("case_count").expect("cc"), 2);
        assert_eq!(row.try_get::<String, _>("status").expect("st"), "RUNNING");
        let task = spy.last().expect("dispatched");
        assert_eq!(task.report_id, report_id); // report_id 透传给执行节点
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
}
