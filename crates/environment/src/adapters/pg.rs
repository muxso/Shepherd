use std::collections::BTreeMap;

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{Environment, NewEnvironment};
use crate::ports::{EnvironmentRepository, RepoError};

#[derive(Clone)]
pub struct PgEnvironmentRepository {
    pool: PgPool,
}

impl PgEnvironmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn headers_to_json(headers: &[(String, String)]) -> serde_json::Value {
    serde_json::Value::Array(
        headers
            .iter()
            .map(|(n, v)| serde_json::json!({"name": n, "value": v}))
            .collect(),
    )
}

fn vars_to_json(vars: &BTreeMap<String, String>) -> serde_json::Value {
    serde_json::Value::Object(
        vars.iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect(),
    )
}

fn json_to_headers(v: &serde_json::Value) -> Vec<(String, String)> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|h| {
                    let name = h.get("name")?.as_str()?.to_string();
                    let value = h.get("value").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    Some((name, value))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn json_to_vars(v: &serde_json::Value) -> BTreeMap<String, String> {
    v.as_object()
        .map(|obj| {
            obj.iter()
                .map(|(k, val)| {
                    let s = match val {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), s)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn row_to_env(row: &sqlx::postgres::PgRow) -> Result<Environment, RepoError> {
    let headers: serde_json::Value = row.try_get("headers").map_err(map_err)?;
    let variables: serde_json::Value = row.try_get("variables").map_err(map_err)?;
    Ok(Environment {
        id: row.try_get("id").map_err(map_err)?,
        project_id: row.try_get("project_id").map_err(map_err)?,
        name: row.try_get("name").map_err(map_err)?,
        base_url: row.try_get("base_url").map_err(map_err)?,
        headers: json_to_headers(&headers),
        variables: json_to_vars(&variables),
        enabled: row.try_get("enabled").map_err(map_err)?,
    })
}

const COLS: &str = "id, project_id, name, base_url, headers, variables, enabled";

#[async_trait]
impl EnvironmentRepository for PgEnvironmentRepository {
    async fn insert(&self, e: &NewEnvironment) -> Result<Environment, RepoError> {
        let row = sqlx::query(&format!(
            "INSERT INTO ms_environment (project_id, name, base_url, headers, variables, enabled) \
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING {COLS}"
        ))
        .bind(&e.project_id)
        .bind(&e.name)
        .bind(&e.base_url)
        .bind(headers_to_json(&e.headers))
        .bind(vars_to_json(&e.variables))
        .bind(e.enabled)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_env(&row)
    }

    async fn get(&self, id: &str) -> Result<Option<Environment>, RepoError> {
        let row = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_environment WHERE id = $1 AND deleted = false"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_env).transpose()
    }

    async fn list_by_project(&self, project_id: &str) -> Result<Vec<Environment>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_environment WHERE project_id = $1 AND deleted = false ORDER BY id"
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_env).collect()
    }

    async fn update(&self, id: &str, e: &NewEnvironment) -> Result<Option<Environment>, RepoError> {
        // project_id 不可变:故意不在 SET 里。
        let row = sqlx::query(&format!(
            "UPDATE ms_environment SET name = $2, base_url = $3, headers = $4, variables = $5, enabled = $6 \
             WHERE id = $1 AND deleted = false RETURNING {COLS}"
        ))
        .bind(id)
        .bind(&e.name)
        .bind(&e.base_url)
        .bind(headers_to_json(&e.headers))
        .bind(vars_to_json(&e.variables))
        .bind(e.enabled)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_env).transpose()
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError> {
        let res = sqlx::query("UPDATE ms_environment SET deleted = true WHERE id = $1 AND deleted = false")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }
}
