use std::collections::BTreeMap;

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{FunctionalCase, NewFunctionalCase};
use crate::ports::{CaseRepository, CaseRequirement, CoverageCase, RepoError};

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

fn fields_to_json(f: &BTreeMap<String, String>) -> serde_json::Value {
    serde_json::Value::Object(
        f.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect(),
    )
}

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

const COLS: &str = "id, project_id, name, module, priority, status, custom_fields, steps, created_by";

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
        created_by: r.try_get("created_by").ok().flatten(),
    })
}

#[async_trait]
impl CaseRepository for PgCaseRepository {
    async fn insert(&self, c: &NewFunctionalCase) -> Result<FunctionalCase, RepoError> {
        let row = sqlx::query(&format!(
            "INSERT INTO ms_functional_case (project_id, name, module, priority, status, custom_fields, steps, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {COLS}"
        ))
        .bind(&c.project_id)
        .bind(&c.name)
        .bind(&c.module)
        .bind(&c.priority)
        .bind(&c.status)
        .bind(fields_to_json(&c.custom_fields))
        .bind(serde_json::to_value(&c.steps).unwrap_or_else(|_| serde_json::json!([])))
        .bind(&c.created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_case(&row)
    }

    async fn update(&self, id: &str, c: &NewFunctionalCase) -> Result<Option<FunctionalCase>, RepoError> {
        let row = sqlx::query(&format!(
            "UPDATE ms_functional_case \
             SET name = $2, module = $3, priority = $4, status = $5, custom_fields = $6, steps = $7 \
             WHERE id = $1 AND NOT deleted RETURNING {COLS}"
        ))
        .bind(id)
        .bind(&c.name)
        .bind(&c.module)
        .bind(&c.priority)
        .bind(&c.status)
        .bind(fields_to_json(&c.custom_fields))
        .bind(serde_json::to_value(&c.steps).unwrap_or_else(|_| serde_json::json!([])))
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_case).transpose()
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError> {
        let res = sqlx::query("UPDATE ms_functional_case SET deleted = true WHERE id = $1 AND NOT deleted")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
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

    async fn link_requirement_case(
        &self,
        requirement_id: &str,
        criterion_index: i32,
        functional_case_id: &str,
        project_id: &str,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO ms_requirement_case (requirement_id, criterion_index, functional_case_id, project_id) \
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(requirement_id)
        .bind(criterion_index)
        .bind(functional_case_id)
        .bind(project_id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn unlink_requirement_case(
        &self,
        requirement_id: &str,
        criterion_index: i32,
        functional_case_id: &str,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "DELETE FROM ms_requirement_case \
             WHERE requirement_id = $1 AND criterion_index = $2 AND functional_case_id = $3",
        )
        .bind(requirement_id)
        .bind(criterion_index)
        .bind(functional_case_id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn cases_for_requirement(&self, requirement_id: &str) -> Result<Vec<CoverageCase>, RepoError> {
        let rows = sqlx::query(
            "SELECT rc.criterion_index, rc.functional_case_id, c.name, c.module, c.priority \
             FROM ms_requirement_case rc JOIN ms_functional_case c ON c.id = rc.functional_case_id \
             WHERE rc.requirement_id = $1 AND NOT c.deleted \
             ORDER BY rc.criterion_index, c.name",
        )
        .bind(requirement_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter()
            .map(|r| {
                Ok(CoverageCase {
                    criterion_index: r.try_get("criterion_index").map_err(map_err)?,
                    case_id: r.try_get("functional_case_id").map_err(map_err)?,
                    case_name: r.try_get("name").map_err(map_err)?,
                    module: r.try_get("module").map_err(map_err)?,
                    priority: r.try_get("priority").map_err(map_err)?,
                })
            })
            .collect()
    }

    async fn requirements_for_case(&self, functional_case_id: &str) -> Result<Vec<CaseRequirement>, RepoError> {
        let rows = sqlx::query(
            "SELECT rc.requirement_id, rc.criterion_index, r.title \
             FROM ms_requirement_case rc JOIN ms_requirement r ON r.id = rc.requirement_id \
             WHERE rc.functional_case_id = $1 AND NOT r.deleted \
             ORDER BY r.title, rc.criterion_index",
        )
        .bind(functional_case_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter()
            .map(|r| {
                Ok(CaseRequirement {
                    requirement_id: r.try_get("requirement_id").map_err(map_err)?,
                    requirement_title: r.try_get("title").map_err(map_err)?,
                    criterion_index: r.try_get("criterion_index").map_err(map_err)?,
                })
            })
            .collect()
    }
}
