//! PostgreSQL 功能用例仓储(表 ms_functional_case)。custom_fields 存 jsonb 对象。

use std::collections::BTreeMap;

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{FunctionalCase, NewFunctionalCase};
use crate::ports::{CaseRepository, RepoError};

#[derive(Clone)]
pub struct PgCaseRepository {
    pool: PgPool,
}

impl PgCaseRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

/// BTreeMap → jsonb 对象(全字符串值)。
fn fields_to_json(f: &BTreeMap<String, String>) -> serde_json::Value {
    serde_json::Value::Object(
        f.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect(),
    )
}

/// jsonb 对象 → BTreeMap(非字符串值用紧凑 JSON 串表示;非对象回落空)。
fn json_to_fields(v: &serde_json::Value) -> BTreeMap<String, String> {
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

const COLS: &str = "id, project_id, name, module, priority, status, custom_fields, steps";

fn row_to_case(r: &sqlx::postgres::PgRow) -> Result<FunctionalCase, RepoError> {
    let custom: serde_json::Value = r.try_get("custom_fields").map_err(map_err)?;
    let steps_json: serde_json::Value = r.try_get("steps").map_err(map_err)?;
    Ok(FunctionalCase {
        id: r.try_get("id").map_err(map_err)?,
        project_id: r.try_get("project_id").map_err(map_err)?,
        name: r.try_get("name").map_err(map_err)?,
        module: r.try_get("module").map_err(map_err)?,
        priority: r.try_get("priority").map_err(map_err)?,
        status: r.try_get("status").map_err(map_err)?,
        custom_fields: json_to_fields(&custom),
        steps: serde_json::from_value(steps_json).unwrap_or_default(),
    })
}

#[async_trait]
impl CaseRepository for PgCaseRepository {
    async fn insert(&self, c: &NewFunctionalCase) -> Result<FunctionalCase, RepoError> {
        let row = sqlx::query(&format!(
            "INSERT INTO ms_functional_case (project_id, name, module, priority, status, custom_fields, steps) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {COLS}"
        ))
        .bind(&c.project_id)
        .bind(&c.name)
        .bind(&c.module)
        .bind(&c.priority)
        .bind(&c.status)
        .bind(fields_to_json(&c.custom_fields))
        .bind(serde_json::to_value(&c.steps).unwrap_or_else(|_| serde_json::json!([])))
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_case(&row)
    }

    async fn list_by_project(&self, project_id: &str) -> Result<Vec<FunctionalCase>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_functional_case WHERE project_id = $1 AND NOT deleted ORDER BY id"
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_case).collect()
    }

    async fn get(&self, id: &str) -> Result<Option<FunctionalCase>, RepoError> {
        let row = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_functional_case WHERE id = $1 AND NOT deleted"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_case).transpose()
    }
}
