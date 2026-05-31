//! PostgreSQL 实现的 `RequirementRepository`。
//!
//! 聚合落在两张表:`ms_requirement`(元信息)+ `ms_requirement_version`(不可变版本快照,
//! `acceptance_criteria` 用 `text[]`)。"项目内未删除标题唯一"由部分唯一索引在 DB 层兜底;
//! `save` 对版本用 `ON CONFLICT DO NOTHING`(版本不可变,只追加不改写)。
//!
//! 集成测试 `#[ignore]`,需 DATABASE_URL:
//!   `DATABASE_URL=postgres://... cargo test -p requirement --features pg -- --ignored`

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{
    AcceptanceCriterion, NewRequirement, Requirement, RequirementStatus, RequirementVersion,
};
use crate::ports::{RepoError, RequirementRepository};

#[derive(Clone)]
pub struct PgRequirementRepository {
    pool: PgPool,
}

impl PgRequirementRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 由元信息行 + 加载版本快照组装完整聚合。
    async fn assemble(&self, meta: &sqlx::postgres::PgRow) -> Result<Requirement, RepoError> {
        let id: String = meta.try_get("id").map_err(map_err)?;
        let baseline_i: i32 = meta.try_get("baseline_version").map_err(map_err)?;
        let status_s: String = meta.try_get("status").map_err(map_err)?;

        let vrows = sqlx::query(
            "SELECT version, description, acceptance_criteria FROM ms_requirement_version \
             WHERE requirement_id = $1 ORDER BY version",
        )
        .bind(&id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;

        let mut versions = Vec::with_capacity(vrows.len());
        for v in &vrows {
            let version: i32 = v.try_get("version").map_err(map_err)?;
            let description: String = v.try_get("description").map_err(map_err)?;
            let criteria: Vec<String> = v.try_get("acceptance_criteria").map_err(map_err)?;
            versions.push(RequirementVersion {
                version: version as u32,
                description,
                acceptance_criteria: criteria
                    .into_iter()
                    .map(|text| AcceptanceCriterion { text })
                    .collect(),
            });
        }

        Ok(Requirement {
            id,
            project_id: meta.try_get("project_id").map_err(map_err)?,
            title: meta.try_get("title").map_err(map_err)?,
            status: RequirementStatus::parse(&status_s).unwrap_or(RequirementStatus::Draft),
            baseline_version: baseline_i as u32,
            versions,
            deleted: meta.try_get("deleted").map_err(map_err)?,
        })
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn criteria_to_vec(v: &RequirementVersion) -> Vec<String> {
    v.acceptance_criteria.iter().map(|c| c.text.clone()).collect()
}

const META_COLS: &str = "id, project_id, title, status, baseline_version, deleted";

#[async_trait]
impl RequirementRepository for PgRequirementRepository {
    async fn find_active_by_title(
        &self,
        project_id: &str,
        title: &str,
    ) -> Result<Option<Requirement>, RepoError> {
        let row = sqlx::query(&format!(
            "SELECT {META_COLS} FROM ms_requirement \
             WHERE project_id = $1 AND title = $2 AND deleted = false"
        ))
        .bind(project_id)
        .bind(title)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        match row {
            Some(r) => Ok(Some(self.assemble(&r).await?)),
            None => Ok(None),
        }
    }

    async fn insert(&self, new: &NewRequirement) -> Result<Requirement, RepoError> {
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        let id: String = sqlx::query(
            "INSERT INTO ms_requirement (project_id, title) VALUES ($1, $2) RETURNING id",
        )
        .bind(&new.project_id)
        .bind(&new.title)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_err)?
        .try_get("id")
        .map_err(map_err)?;

        let req = Requirement::create(&id, new);
        let v1 = req.latest(); // 版本 1
        sqlx::query(
            "INSERT INTO ms_requirement_version (requirement_id, version, description, acceptance_criteria) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(&id)
        .bind(v1.version as i32)
        .bind(&v1.description)
        .bind(criteria_to_vec(v1))
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        tx.commit().await.map_err(map_err)?;
        Ok(req)
    }

    async fn get(&self, id: &str) -> Result<Option<Requirement>, RepoError> {
        let row = sqlx::query(&format!("SELECT {META_COLS} FROM ms_requirement WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?;
        match row {
            Some(r) => Ok(Some(self.assemble(&r).await?)),
            None => Ok(None),
        }
    }

    async fn count_active(&self, project_id: &str) -> Result<u64, RepoError> {
        let row = sqlx::query(
            "SELECT count(*) AS n FROM ms_requirement WHERE project_id = $1 AND deleted = false",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        let n: i64 = row.try_get("n").map_err(map_err)?;
        Ok(n as u64)
    }

    async fn list_active(
        &self,
        project_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<Requirement>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {META_COLS} FROM ms_requirement \
             WHERE project_id = $1 AND deleted = false ORDER BY seq LIMIT $2 OFFSET $3"
        ))
        .bind(project_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            out.push(self.assemble(r).await?);
        }
        Ok(out)
    }

    async fn save(&self, requirement: &Requirement) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        sqlx::query(
            "UPDATE ms_requirement SET title = $2, status = $3, baseline_version = $4, deleted = $5 \
             WHERE id = $1",
        )
        .bind(&requirement.id)
        .bind(&requirement.title)
        .bind(requirement.status.as_str())
        .bind(requirement.baseline_version as i32)
        .bind(requirement.deleted)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        // 版本不可变:只追加尚未落库的版本,已存在的不改写。
        for v in &requirement.versions {
            sqlx::query(
                "INSERT INTO ms_requirement_version (requirement_id, version, description, acceptance_criteria) \
                 VALUES ($1, $2, $3, $4) ON CONFLICT (requirement_id, version) DO NOTHING",
            )
            .bind(&requirement.id)
            .bind(v.version as i32)
            .bind(&v.description)
            .bind(criteria_to_vec(v))
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        }

        tx.commit().await.map_err(map_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_versioning_baseline_and_soft_delete() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::query("TRUNCATE ms_requirement, ms_requirement_version")
            .execute(&pool)
            .await
            .expect("truncate");

        let repo = PgRequirementRepository::new(pool.clone());
        let nu = NewRequirement::new("p1", "登录", "用邮箱登录", &["正确凭证登录".to_string()])
            .expect("valid");

        // insert → v1 / baseline 1
        let r = repo.insert(&nu).await.expect("insert");
        let mut got = repo.get(&r.id).await.expect("get").expect("some");
        assert_eq!(got.latest_version(), 1);
        assert_eq!(got.baseline_version, 1);
        assert_eq!(got.baseline().acceptance_criteria[0].text, "正确凭证登录");

        // revise → v2,baseline 仍 1
        got.revise("v2 描述", vec![AcceptanceCriterion { text: "新增标准".into() }]).expect("revise");
        repo.save(&got).await.expect("save");
        let reloaded = repo.get(&r.id).await.expect("get").expect("some");
        assert_eq!(reloaded.latest_version(), 2);
        assert_eq!(reloaded.baseline_version, 1);
        // v1 未被改写
        assert_eq!(reloaded.version(1).expect("v1").acceptance_criteria[0].text, "正确凭证登录");

        // set baseline → 2
        let mut r2 = reloaded;
        r2.set_baseline(2).expect("baseline");
        repo.save(&r2).await.expect("save");
        assert_eq!(repo.get(&r.id).await.expect("g").expect("s").baseline_version, 2);

        // soft delete 释放标题 → 可重建同名
        assert!(repo.find_active_by_title("p1", "登录").await.expect("q").is_some());
        r2.soft_delete();
        repo.save(&r2).await.expect("save");
        assert!(repo.find_active_by_title("p1", "登录").await.expect("q").is_none());
        assert!(repo.insert(&nu).await.is_ok());
    }
}
