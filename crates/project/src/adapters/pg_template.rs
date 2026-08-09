//! (project_id, kind, name) uniqueness is backstopped at the DB level by the UNIQUE constraint
//! from migration 0085.

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{NewTemplate, Template};
use crate::ports::{RepoError, TemplateRepository};

#[derive(Clone)]
pub struct PgTemplateRepository {
    pool: PgPool,
}

impl PgTemplateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn row_to_template(row: &sqlx::postgres::PgRow) -> Result<Template, RepoError> {
    Ok(Template {
        id: row.try_get("id").map_err(map_err)?,
        project_id: row.try_get("project_id").map_err(map_err)?,
        kind: row.try_get("kind").map_err(map_err)?,
        name: row.try_get("name").map_err(map_err)?,
        config: row.try_get("config").map_err(map_err)?,
        created_by: row.try_get("created_by").map_err(map_err)?,
        created_at_ms: row.try_get("created_at_ms").map_err(map_err)?,
        updated_at_ms: row.try_get("updated_at_ms").map_err(map_err)?,
    })
}

const COLS: &str = "id, project_id, kind, name, config, created_by, \
     (extract(epoch from created_at) * 1000)::bigint AS created_at_ms, \
     (extract(epoch from updated_at) * 1000)::bigint AS updated_at_ms";

#[async_trait]
impl TemplateRepository for PgTemplateRepository {
    async fn insert(&self, t: &NewTemplate) -> Result<Template, RepoError> {
        let row = sqlx::query(&format!(
            "INSERT INTO ms_template (project_id, kind, name, config, created_by) \
             VALUES ($1, $2, $3, $4, $5) RETURNING {COLS}"
        ))
        .bind(&t.project_id)
        .bind(&t.kind)
        .bind(&t.name)
        .bind(&t.config)
        .bind(&t.created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_template(&row)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Template>, RepoError> {
        let row = sqlx::query(&format!("SELECT {COLS} FROM ms_template WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?;
        row.as_ref().map(row_to_template).transpose()
    }

    async fn find_by_name(
        &self,
        project_id: &str,
        kind: &str,
        name: &str,
    ) -> Result<Option<Template>, RepoError> {
        let row = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_template WHERE project_id = $1 AND kind = $2 AND name = $3"
        ))
        .bind(project_id)
        .bind(kind)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_template).transpose()
    }

    async fn update(&self, t: &Template) -> Result<Option<Template>, RepoError> {
        let row = sqlx::query(&format!(
            "UPDATE ms_template SET name = $2, config = $3, updated_at = now() \
             WHERE id = $1 RETURNING {COLS}"
        ))
        .bind(&t.id)
        .bind(&t.name)
        .bind(&t.config)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_template).transpose()
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError> {
        let res = sqlx::query("DELETE FROM ms_template WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn list(&self, project_id: &str, kind: Option<&str>) -> Result<Vec<Template>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_template \
             WHERE project_id = $1 AND ($2::text IS NULL OR kind = $2) \
             ORDER BY created_at, name"
        ))
        .bind(project_id)
        .bind(kind)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_template).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL pointing to a PostgreSQL instance"]
    async fn pg_template_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::query("TRUNCATE ms_template").execute(&pool).await.expect("truncate");

        let repo = PgTemplateRepository::new(pool.clone());
        let nu = NewTemplate::new("p1", "requirement", "default", json!({"fields": [1]}), "admin")
            .expect("valid");

        let t = repo.insert(&nu).await.expect("insert");
        assert!(t.created_at_ms > 0);
        assert_eq!(t.config, json!({"fields": [1]}));
        assert!(repo.find_by_name("p1", "requirement", "default").await.expect("q").is_some());

        repo.insert(&NewTemplate::new("p1", "bug", "default", json!({}), "admin").expect("valid"))
            .await
            .expect("other kind");
        assert_eq!(repo.list("p1", None).await.expect("list").len(), 2);
        assert_eq!(repo.list("p1", Some("requirement")).await.expect("list").len(), 1);

        let mut renamed = t.clone();
        renamed.name = "rename".to_string();
        renamed.config = json!({"a": true});
        let after = repo.update(&renamed).await.expect("update").expect("exists");
        assert_eq!(after.name, "rename");
        assert_eq!(after.config, json!({"a": true}));
        assert!(after.updated_at_ms >= after.created_at_ms);
        assert!(repo
            .update(&Template { id: "nope".into(), ..renamed })
            .await
            .expect("q")
            .is_none());

        assert!(repo.delete(&t.id).await.expect("delete"));
        assert!(!repo.delete(&t.id).await.expect("re-delete"));
        assert!(repo.find_by_id(&t.id).await.expect("q").is_none());
    }
}
