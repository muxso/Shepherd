//! PostgreSQL 压测报告存储(表 `ms_perf_report`)。
//!
//! `create` 落 RUNNING 行并返回 id;`finish` 跑完回写聚合指标(延迟分位存 jsonb);
//! `get_json` 读出整行为 camelCase JSON(供 HTTP 读端点直接返回)。SQL 收敛在此适配器,
//! 组装根不碰裸 sqlx(与本仓约定一致)。

use serde_json::{json, Value};
use sqlx::{PgPool, Row};

use crate::domain::LoadReport;

#[derive(Clone)]
pub struct PgPerfReportStore {
    pool: PgPool,
}

impl PgPerfReportStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 落 RUNNING 报告,返回生成的 id。
    pub async fn create(
        &self,
        project_id: &str,
        method: &str,
        url: &str,
        concurrency: i32,
        iterations: i32,
    ) -> Result<String, sqlx::Error> {
        let row = sqlx::query(
            "INSERT INTO ms_perf_report (project_id, method, url, concurrency, iterations) \
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(project_id)
        .bind(method)
        .bind(url)
        .bind(concurrency)
        .bind(iterations)
        .fetch_one(&self.pool)
        .await?;
        row.try_get("id")
    }

    /// 跑完回写:状态置 COMPLETED + 聚合指标(error_rate 表征质量,延迟分位存 jsonb)。
    pub async fn finish(&self, id: &str, report: &LoadReport) -> Result<(), sqlx::Error> {
        let latency = serde_json::to_value(report.latency).unwrap_or_else(|_| json!({}));
        sqlx::query(
            "UPDATE ms_perf_report SET status='COMPLETED', total=$2, success=$3, failed=$4, \
             error_rate=$5, throughput_rps=$6, elapsed_ms=$7, latency=$8 WHERE id=$1",
        )
        .bind(id)
        .bind(report.total as i32)
        .bind(report.success as i32)
        .bind(report.failed as i32)
        .bind(report.error_rate)
        .bind(report.throughput_rps)
        .bind(report.elapsed_ms as i64)
        .bind(latency)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 读整行为 camelCase JSON;不存在返回 None。
    pub async fn get_json(&self, id: &str) -> Result<Option<Value>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, project_id, method, url, concurrency, iterations, status, total, success, \
             failed, error_rate, throughput_rps, elapsed_ms, latency FROM ms_perf_report WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            json!({
                "id": r.try_get::<String, _>("id").unwrap_or_default(),
                "projectId": r.try_get::<String, _>("project_id").unwrap_or_default(),
                "method": r.try_get::<String, _>("method").unwrap_or_default(),
                "url": r.try_get::<String, _>("url").unwrap_or_default(),
                "concurrency": r.try_get::<i32, _>("concurrency").unwrap_or_default(),
                "iterations": r.try_get::<i32, _>("iterations").unwrap_or_default(),
                "status": r.try_get::<String, _>("status").unwrap_or_default(),
                "total": r.try_get::<i32, _>("total").unwrap_or_default(),
                "success": r.try_get::<i32, _>("success").unwrap_or_default(),
                "failed": r.try_get::<i32, _>("failed").unwrap_or_default(),
                "errorRate": r.try_get::<f64, _>("error_rate").unwrap_or_default(),
                "throughputRps": r.try_get::<f64, _>("throughput_rps").unwrap_or_default(),
                "elapsedMs": r.try_get::<i64, _>("elapsed_ms").unwrap_or_default(),
                "latency": r.try_get::<Value, _>("latency").unwrap_or_else(|_| json!({})),
            })
        }))
    }
}
