use std::collections::BTreeMap;

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{FunctionalCase, NewFunctionalCase};
use crate::ports::{
    CaseBugRef, CaseChange, CaseDependencyRef, CasePlanRef, CaseRepository, CaseRequirement,
    CaseReviewRef, CoverageCase, RepoError,
};

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

const COLS: &str = "id, project_id, COALESCE(num, 0) AS num, name, module, priority, status, \
     tags, custom_fields, steps, created_by, created_at::text AS created_at, \
     updated_at::text AS updated_at";

fn row_to_case(r: &sqlx::postgres::PgRow) -> Result<FunctionalCase, RepoError> {
    let custom: serde_json::Value = r.try_get("custom_fields").map_err(map_err)?;
    let steps_json: serde_json::Value = r.try_get("steps").map_err(map_err)?;
    let tags_json: serde_json::Value = r.try_get("tags").map_err(map_err)?;
    Ok(FunctionalCase {
        id: r.try_get("id").map_err(map_err)?,
        project_id: r.try_get("project_id").map_err(map_err)?,
        num: r.try_get("num").map_err(map_err)?,
        name: r.try_get("name").map_err(map_err)?,
        module: r.try_get("module").map_err(map_err)?,
        priority: r.try_get("priority").map_err(map_err)?,
        status: r.try_get("status").map_err(map_err)?,
        tags: serde_json::from_value(tags_json).unwrap_or_default(),
        custom_fields: json_to_fields(&custom),
        steps: serde_json::from_value(steps_json).unwrap_or_default(),
        created_by: r.try_get("created_by").ok().flatten(),
        created_at: r.try_get("created_at").unwrap_or_default(),
        updated_at: r.try_get("updated_at").unwrap_or_default(),
    })
}

#[async_trait]
impl CaseRepository for PgCaseRepository {
    async fn insert(&self, c: &NewFunctionalCase) -> Result<FunctionalCase, RepoError> {
        let row = sqlx::query(&format!(
            "INSERT INTO ms_functional_case (project_id, num, name, module, priority, status, tags, custom_fields, steps, created_by) \
             VALUES ($1, (SELECT COALESCE(MAX(num) + 1, 100001) FROM ms_functional_case WHERE project_id = $1), \
                     $2, $3, $4, $5, $6, $7, $8, $9) RETURNING {COLS}"
        ))
        .bind(&c.project_id)
        .bind(&c.name)
        .bind(&c.module)
        .bind(&c.priority)
        .bind(&c.status)
        .bind(serde_json::to_value(&c.tags).unwrap_or_else(|_| serde_json::json!([])))
        .bind(fields_to_json(&c.custom_fields))
        .bind(serde_json::to_value(&c.steps).unwrap_or_else(|_| serde_json::json!([])))
        .bind(&c.created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_case(&row)
    }

    async fn update(
        &self,
        id: &str,
        c: &NewFunctionalCase,
    ) -> Result<Option<FunctionalCase>, RepoError> {
        let row = sqlx::query(&format!(
            "UPDATE ms_functional_case \
             SET name = $2, module = $3, priority = $4, status = $5, tags = $6, custom_fields = $7, steps = $8, updated_at = now() \
             WHERE id = $1 AND NOT deleted RETURNING {COLS}"
        ))
        .bind(id)
        .bind(&c.name)
        .bind(&c.module)
        .bind(&c.priority)
        .bind(&c.status)
        .bind(serde_json::to_value(&c.tags).unwrap_or_else(|_| serde_json::json!([])))
        .bind(fields_to_json(&c.custom_fields))
        .bind(serde_json::to_value(&c.steps).unwrap_or_else(|_| serde_json::json!([])))
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_case).transpose()
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError> {
        let res = sqlx::query(
            "UPDATE ms_functional_case SET deleted = true WHERE id = $1 AND NOT deleted",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn list_by_project(&self, project_id: &str) -> Result<Vec<FunctionalCase>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_functional_case WHERE project_id = $1 AND NOT deleted ORDER BY num DESC, id"
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

    async fn cases_for_requirement(
        &self,
        requirement_id: &str,
    ) -> Result<Vec<CoverageCase>, RepoError> {
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

    async fn requirements_for_case(
        &self,
        functional_case_id: &str,
    ) -> Result<Vec<CaseRequirement>, RepoError> {
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

    async fn record_changes(
        &self,
        case_id: &str,
        changes: &[(String, String, String)],
        actor: &str,
    ) -> Result<(), RepoError> {
        for (field, old, new) in changes {
            sqlx::query(
                "INSERT INTO ms_functional_case_change (case_id, field, old_value, new_value, actor) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(case_id)
            .bind(field)
            .bind(old)
            .bind(new)
            .bind(actor)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        }
        Ok(())
    }

    async fn list_changes(&self, case_id: &str) -> Result<Vec<CaseChange>, RepoError> {
        let rows = sqlx::query(
            "SELECT field, old_value, new_value, actor, created_at::text AS created_at \
             FROM ms_functional_case_change WHERE case_id = $1 ORDER BY created_at DESC, id DESC",
        )
        .bind(case_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter()
            .map(|r| {
                Ok(CaseChange {
                    field: r.try_get("field").map_err(map_err)?,
                    old_value: r.try_get("old_value").map_err(map_err)?,
                    new_value: r.try_get("new_value").map_err(map_err)?,
                    actor: r.try_get("actor").map_err(map_err)?,
                    created_at: r.try_get("created_at").map_err(map_err)?,
                })
            })
            .collect()
    }

    async fn add_dependency(
        &self,
        project_id: &str,
        case_id: &str,
        depends_on_id: &str,
        created_by: &str,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO ms_case_dependency (project_id, case_id, depends_on_id, created_by) \
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(project_id)
        .bind(case_id)
        .bind(depends_on_id)
        .bind(created_by)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn remove_dependency(
        &self,
        case_id: &str,
        depends_on_id: &str,
    ) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM ms_case_dependency WHERE case_id = $1 AND depends_on_id = $2")
            .bind(case_id)
            .bind(depends_on_id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn dependencies_for_case(
        &self,
        case_id: &str,
        reverse: bool,
    ) -> Result<Vec<CaseDependencyRef>, RepoError> {
        // Forward lists preconditions (edges out of the case); reverse lists
        // cases that declare this one as their precondition.
        let sql = if reverse {
            "SELECT c.id, COALESCE(c.num, 0) AS num, c.name, COALESCE(c.created_by, '') AS created_by \
             FROM ms_case_dependency d JOIN ms_functional_case c ON c.id = d.case_id \
             WHERE d.depends_on_id = $1 AND NOT c.deleted ORDER BY c.num"
        } else {
            "SELECT c.id, COALESCE(c.num, 0) AS num, c.name, COALESCE(c.created_by, '') AS created_by \
             FROM ms_case_dependency d JOIN ms_functional_case c ON c.id = d.depends_on_id \
             WHERE d.case_id = $1 AND NOT c.deleted ORDER BY c.num"
        };
        let rows = sqlx::query(sql).bind(case_id).fetch_all(&self.pool).await.map_err(map_err)?;
        rows.iter()
            .map(|r| {
                Ok(CaseDependencyRef {
                    case_id: r.try_get("id").map_err(map_err)?,
                    num: r.try_get("num").map_err(map_err)?,
                    name: r.try_get("name").map_err(map_err)?,
                    created_by: r.try_get("created_by").map_err(map_err)?,
                })
            })
            .collect()
    }

    async fn bugs_for_case(&self, case_id: &str) -> Result<Vec<CaseBugRef>, RepoError> {
        let rows = sqlx::query(
            "SELECT b.id, b.title, b.status, COALESCE(b.created_by, '') AS created_by, \
                    COALESCE(b.custom_fields->>'处理人', b.custom_fields->>'handleUser', '') AS handler \
             FROM ms_bug_relation r JOIN ms_bug b ON b.id = r.bug_id \
             WHERE r.kind = 'FUNCTIONAL_CASE' AND r.target_id = $1 AND NOT b.deleted \
             ORDER BY b.created_at DESC",
        )
        .bind(case_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter()
            .map(|r| {
                Ok(CaseBugRef {
                    bug_id: r.try_get("id").map_err(map_err)?,
                    title: r.try_get("title").map_err(map_err)?,
                    status: r.try_get("status").map_err(map_err)?,
                    created_by: r.try_get("created_by").map_err(map_err)?,
                    handler: r.try_get("handler").map_err(map_err)?,
                })
            })
            .collect()
    }

    async fn reviews_for_case(&self, case_id: &str) -> Result<Vec<CaseReviewRef>, RepoError> {
        let rows = sqlx::query(
            "SELECT rv.id, COALESCE(s.status, 'PENDING') AS status, rv.created_at::text AS created_at \
             FROM ms_case_review rv \
             LEFT JOIN ms_case_review_status s ON s.review_id = rv.id AND s.case_id = $1 \
             WHERE $1 = ANY(rv.case_ids) AND NOT rv.deleted \
             ORDER BY rv.created_at DESC",
        )
        .bind(case_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter()
            .map(|r| {
                Ok(CaseReviewRef {
                    review_id: r.try_get("id").map_err(map_err)?,
                    status: r.try_get("status").map_err(map_err)?,
                    created_at: r.try_get("created_at").map_err(map_err)?,
                })
            })
            .collect()
    }

    async fn plans_for_case(&self, case_id: &str) -> Result<Vec<CasePlanRef>, RepoError> {
        let rows = sqlx::query(
            "SELECT p.id, p.name, COALESCE(pr.name, '') AS project_name, p.archived, \
                    pc.status AS exec_status, COALESCE(pc.executed_at::text, '') AS executed_at \
             FROM ms_test_plan_case pc \
             JOIN ms_test_plan p ON p.id = pc.plan_id \
             LEFT JOIN ms_project pr ON pr.id = p.project_id \
             WHERE pc.case_id = $1 ORDER BY p.created_at DESC",
        )
        .bind(case_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter()
            .map(|r| {
                Ok(CasePlanRef {
                    plan_id: r.try_get("id").map_err(map_err)?,
                    plan_name: r.try_get("name").map_err(map_err)?,
                    project_name: r.try_get("project_name").map_err(map_err)?,
                    archived: r.try_get("archived").map_err(map_err)?,
                    exec_status: r.try_get("exec_status").map_err(map_err)?,
                    executed_at: r.try_get("executed_at").map_err(map_err)?,
                })
            })
            .collect()
    }
}
