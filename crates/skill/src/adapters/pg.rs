use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{NewSkill, Skill};
use crate::ports::{RepoError, SkillRepository};

#[derive(Clone)]
pub struct PgSkillRepository {
    pool: PgPool,
}

impl PgSkillRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

const COLS: &str = "id, project_id, name, description, instructions, includes, enabled, deleted";

fn row_to_skill(row: &sqlx::postgres::PgRow) -> Result<Skill, RepoError> {
    Ok(Skill {
        id: row.try_get("id").map_err(map_err)?,
        project_id: row.try_get("project_id").map_err(map_err)?,
        name: row.try_get("name").map_err(map_err)?,
        description: row.try_get("description").map_err(map_err)?,
        instructions: row.try_get("instructions").map_err(map_err)?,
        includes: row.try_get("includes").map_err(map_err)?,
        enabled: row.try_get("enabled").map_err(map_err)?,
        deleted: row.try_get("deleted").map_err(map_err)?,
    })
}

#[async_trait]
impl SkillRepository for PgSkillRepository {
    async fn find_active_by_name(
        &self,
        project_id: &str,
        name: &str,
    ) -> Result<Option<Skill>, RepoError> {
        let row = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_skill WHERE project_id = $1 AND name = $2 AND deleted = false"
        ))
        .bind(project_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_skill).transpose()
    }

    async fn insert(&self, new: &NewSkill) -> Result<Skill, RepoError> {
        let row = sqlx::query(&format!(
            "INSERT INTO ms_skill (project_id, name, description, instructions, includes) \
             VALUES ($1, $2, $3, $4, $5) RETURNING {COLS}"
        ))
        .bind(&new.project_id)
        .bind(&new.name)
        .bind(&new.description)
        .bind(&new.instructions)
        .bind(&new.includes)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_skill(&row)
    }

    async fn get(&self, id: &str) -> Result<Option<Skill>, RepoError> {
        let row = sqlx::query(&format!("SELECT {COLS} FROM ms_skill WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?;
        row.as_ref().map(row_to_skill).transpose()
    }

    async fn list_active(&self, project_id: &str) -> Result<Vec<Skill>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_skill WHERE project_id = $1 AND deleted = false ORDER BY seq"
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_skill).collect()
    }

    async fn save(&self, skill: &Skill) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE ms_skill SET name = $2, description = $3, instructions = $4, includes = $5, \
             enabled = $6, deleted = $7 WHERE id = $1",
        )
        .bind(&skill.id)
        .bind(&skill.name)
        .bind(&skill.description)
        .bind(&skill.instructions)
        .bind(&skill.includes)
        .bind(skill.enabled)
        .bind(skill.deleted)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_compose_roundtrip() {
        use crate::domain::SkillLibrary;
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::query("TRUNCATE ms_skill").execute(&pool).await.expect("truncate");

        let repo = PgSkillRepository::new(pool.clone());
        let a = repo.insert(&NewSkill::new("p1", "基础", "", "遵循六边形", &[]).expect("v")).await.expect("a");
        let _b = repo
            .insert(&NewSkill::new("p1", "Rust", "", "用 thiserror", &[a.id.clone()]).expect("v"))
            .await
            .expect("b");

        let lib = SkillLibrary::new(repo.list_active("p1").await.expect("list"));
        let comp = lib.compose(&[_b.id.clone()]).expect("compose");
        assert_eq!(comp.skill_ids[0], a.id);
        assert!(comp.instructions.contains("遵循六边形") && comp.instructions.contains("用 thiserror"));
    }
}
