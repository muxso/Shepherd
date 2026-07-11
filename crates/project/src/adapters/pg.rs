//! 同组织未删除项目名唯一由 schema.sql 的部分唯一索引在 DB 层兜底。

use crate::domain::{NewProject, Project};
use crate::ports::{ProjectRepository, RepoError};
use async_trait::async_trait;
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PgProjectRepository {
    pool: PgPool,
}

impl PgProjectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn row_to_project(row: &sqlx::postgres::PgRow) -> Result<Project, RepoError> {
    Ok(Project {
        id: row.try_get("id").map_err(map_err)?,
        organization_id: row.try_get("organization_id").map_err(map_err)?,
        name: row.try_get("name").map_err(map_err)?,
        enable: row.try_get("enable").map_err(map_err)?,
        deleted: row.try_get("deleted").map_err(map_err)?,
    })
}

#[async_trait]
impl ProjectRepository for PgProjectRepository {
    async fn find_active_by_name(
        &self,
        organization_id: &str,
        name: &str,
    ) -> Result<Option<Project>, RepoError> {
        let row = sqlx::query(
            "SELECT id, organization_id, name, enable, deleted FROM ms_project \
             WHERE organization_id = $1 AND name = $2 AND deleted = false",
        )
        .bind(organization_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_project).transpose()
    }

    async fn insert(&self, new_project: &NewProject) -> Result<Project, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_project (organization_id, name) VALUES ($1, $2) \
             RETURNING id, organization_id, name, enable, deleted",
        )
        .bind(&new_project.organization_id)
        .bind(&new_project.name)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_project(&row)
    }

    async fn count_active(&self, organization_id: &str) -> Result<u64, RepoError> {
        let row = sqlx::query(
            "SELECT count(*) AS n FROM ms_project WHERE organization_id = $1 AND deleted = false",
        )
        .bind(organization_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        let n: i64 = row.try_get("n").map_err(map_err)?;
        Ok(n as u64)
    }

    async fn list_active(
        &self,
        organization_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<Project>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, organization_id, name, enable, deleted FROM ms_project \
             WHERE organization_id = $1 AND deleted = false \
             ORDER BY seq LIMIT $2 OFFSET $3",
        )
        .bind(organization_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_project).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_soft_delete_frees_name_and_partial_index_allows_recreate() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::query("TRUNCATE ms_project").execute(&pool).await.expect("truncate");

        let repo = PgProjectRepository::new(pool.clone());
        let nu = NewProject::new("org1", "Demo").expect("valid");

        let p = repo.insert(&nu).await.expect("insert");
        assert!(repo.find_active_by_name("org1", "Demo").await.expect("q").is_some());
        assert_eq!(repo.count_active("org1").await.expect("c"), 1);

        sqlx::query("UPDATE ms_project SET deleted = true WHERE id = $1")
            .bind(&p.id)
            .execute(&pool)
            .await
            .expect("soft delete");
        assert!(repo.find_active_by_name("org1", "Demo").await.expect("q").is_none());
        assert_eq!(repo.count_active("org1").await.expect("c"), 0);

        let again = repo.insert(&nu).await.expect("recreate same name");
        assert_ne!(again.id, p.id);
        assert_eq!(repo.count_active("org1").await.expect("c"), 1);
    }
}
