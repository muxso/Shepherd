//! PostgreSQL 实现的 `VerificationRepository`。
//!
//! 三张表:`ms_verification`(元信息)+ `ms_verification_criterion`(标准快照)+
//! `ms_verification_link`(覆盖链,带 satisfied)。`save` 对覆盖链 upsert(追加 + 更新 satisfied)。
//!
//! 集成测试 `#[ignore]`,需 DATABASE_URL:
//!   `DATABASE_URL=postgres://... cargo test -p verification --features pg -- --ignored`

use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{CoverageLink, NewVerification, Verification, VerifiedCriterion};
use crate::ports::{RepoError, VerificationRepository};

#[derive(Clone)]
pub struct PgVerificationRepository {
    pool: PgPool,
}

impl PgVerificationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn assemble(&self, meta: &sqlx::postgres::PgRow) -> Result<Verification, RepoError> {
        let id: String = meta.try_get("id").map_err(map_err)?;
        let version_i: i32 = meta.try_get("requirement_version").map_err(map_err)?;

        // 覆盖链:criterion_idx -> [CoverageLink]
        let lrows = sqlx::query(
            "SELECT criterion_idx, decomposition_id, task_id, satisfied \
             FROM ms_verification_link WHERE verification_id = $1",
        )
        .bind(&id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let mut links: HashMap<i32, Vec<CoverageLink>> = HashMap::new();
        for r in &lrows {
            let idx: i32 = r.try_get("criterion_idx").map_err(map_err)?;
            links.entry(idx).or_default().push(CoverageLink {
                decomposition_id: r.try_get("decomposition_id").map_err(map_err)?,
                task_id: r.try_get("task_id").map_err(map_err)?,
                satisfied: r.try_get("satisfied").map_err(map_err)?,
            });
        }

        let crows = sqlx::query(
            "SELECT idx, text FROM ms_verification_criterion WHERE verification_id = $1 ORDER BY idx",
        )
        .bind(&id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let mut criteria = Vec::with_capacity(crows.len());
        for c in &crows {
            let idx: i32 = c.try_get("idx").map_err(map_err)?;
            criteria.push(VerifiedCriterion {
                index: idx as u32,
                text: c.try_get("text").map_err(map_err)?,
                links: links.remove(&idx).unwrap_or_default(),
            });
        }

        Ok(Verification {
            id,
            requirement_id: meta.try_get("requirement_id").map_err(map_err)?,
            requirement_version: version_i as u32,
            criteria,
        })
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

const META_COLS: &str = "id, requirement_id, requirement_version";

#[async_trait]
impl VerificationRepository for PgVerificationRepository {
    async fn create(&self, new: &NewVerification) -> Result<Verification, RepoError> {
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        let id: String = sqlx::query(
            "INSERT INTO ms_verification (requirement_id, requirement_version) \
             VALUES ($1, $2) RETURNING id",
        )
        .bind(&new.requirement_id)
        .bind(new.requirement_version as i32)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_err)?
        .try_get("id")
        .map_err(map_err)?;

        for (i, text) in new.criteria.iter().enumerate() {
            sqlx::query(
                "INSERT INTO ms_verification_criterion (verification_id, idx, text) VALUES ($1, $2, $3)",
            )
            .bind(&id)
            .bind(i as i32)
            .bind(text)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(Verification::from_new(&id, new))
    }

    async fn find_by_requirement_version(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Option<Verification>, RepoError> {
        let row = sqlx::query(&format!(
            "SELECT {META_COLS} FROM ms_verification \
             WHERE requirement_id = $1 AND requirement_version = $2"
        ))
        .bind(requirement_id)
        .bind(requirement_version as i32)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        match row {
            Some(r) => Ok(Some(self.assemble(&r).await?)),
            None => Ok(None),
        }
    }

    async fn get(&self, id: &str) -> Result<Option<Verification>, RepoError> {
        let row = sqlx::query(&format!("SELECT {META_COLS} FROM ms_verification WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?;
        match row {
            Some(r) => Ok(Some(self.assemble(&r).await?)),
            None => Ok(None),
        }
    }

    async fn save(&self, verification: &Verification) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        for c in &verification.criteria {
            for l in &c.links {
                sqlx::query(
                    "INSERT INTO ms_verification_link \
                     (verification_id, criterion_idx, decomposition_id, task_id, satisfied) \
                     VALUES ($1, $2, $3, $4, $5) \
                     ON CONFLICT (verification_id, criterion_idx, decomposition_id, task_id) \
                     DO UPDATE SET satisfied = EXCLUDED.satisfied",
                )
                .bind(&verification.id)
                .bind(c.index as i32)
                .bind(&l.decomposition_id)
                .bind(&l.task_id)
                .bind(l.satisfied)
                .execute(&mut *tx)
                .await
                .map_err(map_err)?;
            }
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
    async fn pg_traceability_and_completeness() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::query("TRUNCATE ms_verification, ms_verification_criterion, ms_verification_link")
            .execute(&pool)
            .await
            .expect("truncate");

        let repo = PgVerificationRepository::new(pool.clone());
        let new = NewVerification::new("req1", 1, &["登录成功".into(), "错误密码拒绝".into()])
            .expect("valid");
        let v = repo.create(&new).await.expect("create");
        assert_eq!(repo.get(&v.id).await.expect("g").expect("s").criteria.len(), 2);

        // 覆盖 + 验证一条,另一条留缺口
        let mut v = repo.get(&v.id).await.expect("g").expect("s");
        v.link(0, "d1", "t1").expect("link");
        v.link(1, "d1", "t2").expect("link");
        v.sync_task("d1", "t1", true);
        repo.save(&v).await.expect("save");

        let reloaded = repo.get(&v.id).await.expect("g").expect("s");
        let report = reloaded.report();
        assert!(!report.complete);
        assert_eq!(report.satisfied, 1);
        assert_eq!(report.gaps.len(), 1);
        assert_eq!(report.gaps[0].criterion_index, 1);

        // 补齐 → complete
        let mut v2 = reloaded;
        v2.sync_task("d1", "t2", true);
        repo.save(&v2).await.expect("save");
        assert!(repo.get(&v.id).await.expect("g").expect("s").is_complete());
        assert!(repo.find_by_requirement_version("req1", 1).await.expect("f").is_some());
    }
}
