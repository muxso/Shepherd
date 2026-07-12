use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{MemberRole, NewMember, ProjectMember};
use crate::ports::{ProjectMemberRepository, RepoError};

#[derive(Clone)]
pub struct PgProjectMemberRepository {
    pool: PgPool,
}

impl PgProjectMemberRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn row_to_member(row: &sqlx::postgres::PgRow) -> Result<ProjectMember, RepoError> {
    let role: String = row.try_get("role").map_err(map_err)?;
    Ok(ProjectMember {
        project_id: row.try_get("project_id").map_err(map_err)?,
        user_id: row.try_get("user_id").map_err(map_err)?,
        role: MemberRole::parse(&role).map_err(|e| RepoError::Backend(e.to_string()))?,
        added_at: row.try_get("added_at").map_err(map_err)?,
    })
}

const COLS: &str = "project_id, user_id, role, added_at::text AS added_at";

#[async_trait]
impl ProjectMemberRepository for PgProjectMemberRepository {
    async fn upsert(&self, m: &NewMember) -> Result<ProjectMember, RepoError> {
        let row = sqlx::query(&format!(
            "INSERT INTO ms_project_member (project_id, user_id, role) VALUES ($1, $2, $3) \
             ON CONFLICT (project_id, user_id) DO UPDATE SET role = EXCLUDED.role \
             RETURNING {COLS}"
        ))
        .bind(&m.project_id)
        .bind(&m.user_id)
        .bind(m.role.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_member(&row)
    }

    async fn list(&self, project_id: &str) -> Result<Vec<ProjectMember>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_project_member WHERE project_id = $1 ORDER BY added_at, user_id"
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_member).collect()
    }

    async fn remove(&self, project_id: &str, user_id: &str) -> Result<bool, RepoError> {
        let res =
            sqlx::query("DELETE FROM ms_project_member WHERE project_id = $1 AND user_id = $2")
                .bind(project_id)
                .bind(user_id)
                .execute(&self.pool)
                .await
                .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }
}
