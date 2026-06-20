//! PostgreSQL agent 注册表(表 ms_runner_agent)。token 落库供派发,不入视图。

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{DispatchTarget, ExecutionRecord, NewRunnerAgent, RemoteResult, RunnerAgent};
use crate::ports::{ExecutionStore, PortError, RunnerAgentStore};

#[derive(Clone)]
pub struct PgRunnerAgentStore {
    pool: PgPool,
}

impl PgRunnerAgentStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> PortError {
    PortError::Backend(e.to_string())
}

#[async_trait]
impl RunnerAgentStore for PgRunnerAgentStore {
    async fn insert(&self, a: &NewRunnerAgent) -> Result<RunnerAgent, PortError> {
        let row = sqlx::query(
            "INSERT INTO ms_runner_agent (name, base_url, token, enabled) \
             VALUES ($1, $2, $3, $4) RETURNING id, name, base_url, enabled",
        )
        .bind(&a.name)
        .bind(&a.base_url)
        .bind(&a.token)
        .bind(a.enabled)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(RunnerAgent {
            id: row.try_get("id").map_err(map_err)?,
            name: row.try_get("name").map_err(map_err)?,
            base_url: row.try_get("base_url").map_err(map_err)?,
            enabled: row.try_get("enabled").map_err(map_err)?,
        })
    }

    async fn list(&self) -> Result<Vec<RunnerAgent>, PortError> {
        let rows = sqlx::query(
            "SELECT id, name, base_url, enabled FROM ms_runner_agent WHERE NOT deleted ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.into_iter()
            .map(|r| {
                Ok(RunnerAgent {
                    id: r.try_get("id").map_err(map_err)?,
                    name: r.try_get("name").map_err(map_err)?,
                    base_url: r.try_get("base_url").map_err(map_err)?,
                    enabled: r.try_get("enabled").map_err(map_err)?,
                })
            })
            .collect()
    }

    async fn dispatch_target(&self, id: &str) -> Result<Option<DispatchTarget>, PortError> {
        let row = sqlx::query(
            "SELECT base_url, token FROM ms_runner_agent \
             WHERE id = $1 AND enabled AND NOT deleted",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        match row {
            Some(r) => Ok(Some(DispatchTarget {
                base_url: r.try_get("base_url").map_err(map_err)?,
                token: r.try_get("token").map_err(map_err)?,
            })),
            None => Ok(None),
        }
    }
}

/// PostgreSQL 执行历史(表 ms_runner_execution)。failures 存 jsonb 数组。
#[derive(Clone)]
pub struct PgExecutionStore {
    pool: PgPool,
}

impl PgExecutionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExecutionStore for PgExecutionStore {
    async fn record(
        &self,
        agent_id: &str,
        method: &str,
        url: &str,
        result: &RemoteResult,
    ) -> Result<(), PortError> {
        let failures = serde_json::Value::Array(
            result.failures.iter().map(|f| serde_json::Value::String(f.clone())).collect(),
        );
        sqlx::query(
            "INSERT INTO ms_runner_execution \
             (agent_id, method, url, outcome, status, elapsed_ms, failures) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(agent_id)
        .bind(method)
        .bind(url)
        .bind(&result.outcome)
        .bind(result.status.map(|s| s as i32))
        .bind(result.elapsed_ms.map(|e| e as i64))
        .bind(failures)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn list_by_agent(
        &self,
        agent_id: &str,
        limit: u32,
    ) -> Result<Vec<ExecutionRecord>, PortError> {
        let rows = sqlx::query(
            "SELECT id, agent_id, method, url, outcome, status, elapsed_ms, failures, \
             to_char(executed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS executed_at \
             FROM ms_runner_execution WHERE agent_id = $1 ORDER BY executed_at DESC LIMIT $2",
        )
        .bind(agent_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.into_iter()
            .map(|r| {
                let failures: serde_json::Value = r.try_get("failures").map_err(map_err)?;
                let failures = failures
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                Ok(ExecutionRecord {
                    id: r.try_get("id").map_err(map_err)?,
                    agent_id: r.try_get("agent_id").map_err(map_err)?,
                    method: r.try_get("method").map_err(map_err)?,
                    url: r.try_get("url").map_err(map_err)?,
                    outcome: r.try_get("outcome").map_err(map_err)?,
                    status: r.try_get::<Option<i32>, _>("status").map_err(map_err)?.map(|s| s as u16),
                    elapsed_ms: r.try_get::<Option<i64>, _>("elapsed_ms").map_err(map_err)?.map(|e| e as u64),
                    failures,
                    executed_at: r.try_get("executed_at").map_err(map_err)?,
                })
            })
            .collect()
    }
}
