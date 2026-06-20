//! PostgreSQL agent 注册表(表 ms_runner_agent)。token 落库供派发,不入视图。

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{DispatchTarget, NewRunnerAgent, RunnerAgent};
use crate::ports::{PortError, RunnerAgentStore};

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
