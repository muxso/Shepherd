use async_trait::async_trait;
use sqlx::{PgPool, Row};

use api_runner::{Assertion, HttpMethod, RequestSpec};

use crate::domain::{
    AgentTarget, CaseSpec, DispatchTarget, ExecutionRecord, NewRunnerAgent, RemoteResult,
    RunnerAgent,
};
use crate::ports::{CaseSpecSource, ExecutionStore, PortError, RunnerAgentStore};

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
    async fn insert(
        &self,
        a: &NewRunnerAgent,
        protocols: &[String],
    ) -> Result<RunnerAgent, PortError> {
        let row = sqlx::query(
            "INSERT INTO ms_runner_agent (name, base_url, token, enabled, protocols) \
             VALUES ($1, $2, $3, $4, $5) RETURNING id, name, base_url, enabled, protocols",
        )
        .bind(&a.name)
        .bind(&a.base_url)
        .bind(&a.token)
        .bind(a.enabled)
        .bind(protocols)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(RunnerAgent {
            id: row.try_get("id").map_err(map_err)?,
            name: row.try_get("name").map_err(map_err)?,
            base_url: row.try_get("base_url").map_err(map_err)?,
            enabled: row.try_get("enabled").map_err(map_err)?,
            protocols: row.try_get("protocols").map_err(map_err)?,
        })
    }

    async fn list(&self) -> Result<Vec<RunnerAgent>, PortError> {
        let rows = sqlx::query(
            "SELECT id, name, base_url, enabled, protocols FROM ms_runner_agent \
             WHERE NOT deleted ORDER BY name",
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
                    protocols: r.try_get("protocols").map_err(map_err)?,
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

    async fn agents_for_protocol(&self, protocol: &str) -> Result<Vec<AgentTarget>, PortError> {
        let rows = sqlx::query(
            "SELECT id, name, base_url, token FROM ms_runner_agent \
             WHERE enabled AND NOT deleted AND $1 = ANY(protocols) ORDER BY name",
        )
        .bind(protocol)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.into_iter()
            .map(|r| {
                Ok(AgentTarget {
                    id: r.try_get("id").map_err(map_err)?,
                    name: r.try_get("name").map_err(map_err)?,
                    target: DispatchTarget {
                        base_url: r.try_get("base_url").map_err(map_err)?,
                        token: r.try_get("token").map_err(map_err)?,
                    },
                })
            })
            .collect()
    }

    async fn set_protocols(&self, id: &str, protocols: &[String]) -> Result<bool, PortError> {
        let res =
            sqlx::query("UPDATE ms_runner_agent SET protocols = $2 WHERE id = $1 AND NOT deleted")
                .bind(id)
                .bind(protocols)
                .execute(&self.pool)
                .await
                .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }
}

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
                    status: r
                        .try_get::<Option<i32>, _>("status")
                        .map_err(map_err)?
                        .map(|s| s as u16),
                    elapsed_ms: r
                        .try_get::<Option<i64>, _>("elapsed_ms")
                        .map_err(map_err)?
                        .map(|e| e as u64),
                    failures,
                    executed_at: r.try_get("executed_at").map_err(map_err)?,
                })
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct PgCaseSpecSource {
    pool: PgPool,
}

impl PgCaseSpecSource {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn method_of(m: &str) -> HttpMethod {
    serde_json::from_value(serde_json::Value::String(m.to_uppercase())).unwrap_or(HttpMethod::Get)
}

#[async_trait]
impl CaseSpecSource for PgCaseSpecSource {
    async fn spec_of(&self, case_id: &str) -> Result<Option<CaseSpec>, PortError> {
        let row =
            sqlx::query("SELECT method, url, body, assertions FROM ms_api_case WHERE id = $1")
                .bind(case_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_err)?;
        let Some(r) = row else { return Ok(None) };
        let method: String = r.try_get("method").map_err(map_err)?;
        let url: String = r.try_get("url").map_err(map_err)?;
        let body: Option<String> = r.try_get("body").map_err(map_err)?;
        let assertions_json: serde_json::Value = r.try_get("assertions").map_err(map_err)?;
        let assertions: Vec<Assertion> =
            serde_json::from_value(assertions_json).unwrap_or_default();
        Ok(Some(CaseSpec {
            request: RequestSpec { method: method_of(&method), url, headers: vec![], body },
            assertions,
        }))
    }
}
