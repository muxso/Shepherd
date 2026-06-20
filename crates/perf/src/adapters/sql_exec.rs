//! SQL 协议执行器:对目标 PostgreSQL 反复执行一条查询并计时(协议广度,复用 sqlx)。
//!
//! 与 HTTP(`ApiRunnerExecutor`)并列实现 `RequestExecutor`,故压测引擎(并发、计时、
//! 分位聚合、样本下沉)对 SQL 协议完全复用。一个连接池在多个并发 worker 间共享。
//! 成功 = 查询执行无错(可借 `SELECT 1` 测连通,或真实业务 SQL 测吞吐)。

use async_trait::async_trait;
use sqlx::PgPool;

use crate::ports::RequestExecutor;

pub struct SqlExecutor {
    pool: PgPool,
    query: String,
}

impl SqlExecutor {
    /// 连接目标库并预置待压测的 SQL。`max_conns` 通常设为压测并发数。
    /// 建连超时 5s:目标不可达时快速失败,不阻塞调用方(HTTP 入口)。
    pub async fn connect(
        conn_str: &str,
        query: &str,
        max_conns: u32,
    ) -> Result<Self, sqlx::Error> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(max_conns.max(1))
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(conn_str)
            .await?;
        Ok(Self { pool, query: query.to_string() })
    }

    /// 用已有连接池构造(测试/复用现有池)。
    pub fn with_pool(pool: PgPool, query: &str) -> Self {
        Self { pool, query: query.to_string() }
    }
}

#[async_trait]
impl RequestExecutor for SqlExecutor {
    async fn execute(&self) -> bool {
        sqlx::query(&self.query).execute(&self.pool).await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// 真库集成:对 DATABASE_URL 跑一轮 SELECT 1 压测。需 `-- --ignored` + 真 PG。
    #[tokio::test]
    #[ignore = "需要 PostgreSQL(DATABASE_URL)"]
    async fn sql_load_against_real_db() {
        use crate::adapters::run_load;
        use crate::domain::LoadPlan;

        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        let exec = Arc::new(SqlExecutor::connect(&url, "SELECT 1", 8).await.expect("connect"));
        let plan = LoadPlan::new(8, 200).expect("plan");
        let report = run_load(&plan, exec).await;
        assert_eq!(report.total, 200);
        assert_eq!(report.failed, 0); // SELECT 1 恒成功
        assert!(report.throughput_rps > 0.0);
    }

    /// 坏查询 → execute 返回 false(失败计入报告,不 panic)。
    #[tokio::test]
    #[ignore = "需要 PostgreSQL(DATABASE_URL)"]
    async fn bad_query_counts_as_failure() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        let exec = SqlExecutor::connect(&url, "SELECT no_such_col FROM no_such_table", 2)
            .await
            .expect("connect");
        assert!(!exec.execute().await);
    }
}
