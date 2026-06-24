//! PostgreSQL 实现的 `ProposalRepository`。表 `ms_design_proposal`。
//!
//! 集成测试 `#[ignore]`,需 DATABASE_URL:
//!   `DATABASE_URL=postgres://... cargo test -p design --features pg -- --ignored`

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{Proposal, ProposalStatus};
use crate::ports::{ProposalRepository, RepoError};

#[derive(Clone)]
pub struct PgProposalRepository {
    pool: PgPool,
}

impl PgProposalRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

const COLS: &str = "id, requirement_id, title, design_doc, status, review_comment, revision";

fn row_to_proposal(row: &sqlx::postgres::PgRow) -> Result<Proposal, RepoError> {
    let status_s: String = row.try_get("status").map_err(map_err)?;
    let revision: i32 = row.try_get("revision").map_err(map_err)?;
    Ok(Proposal {
        id: row.try_get("id").map_err(map_err)?,
        requirement_id: row.try_get("requirement_id").map_err(map_err)?,
        title: row.try_get("title").map_err(map_err)?,
        design_doc: row.try_get("design_doc").map_err(map_err)?,
        status: ProposalStatus::parse(&status_s)
            .ok_or_else(|| RepoError::Backend(format!("bad status: {status_s}")))?,
        review_comment: row.try_get("review_comment").map_err(map_err)?,
        revision: revision.max(0) as u32,
    })
}

#[async_trait]
impl ProposalRepository for PgProposalRepository {
    async fn create(&self, requirement_id: &str, title: &str) -> Result<Proposal, RepoError> {
        let row = sqlx::query(&format!(
            "INSERT INTO ms_design_proposal (requirement_id, title) VALUES ($1, $2) RETURNING {COLS}"
        ))
        .bind(requirement_id)
        .bind(title)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_proposal(&row)
    }

    async fn get(&self, id: &str) -> Result<Option<Proposal>, RepoError> {
        let row = sqlx::query(&format!("SELECT {COLS} FROM ms_design_proposal WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?;
        row.as_ref().map(row_to_proposal).transpose()
    }

    async fn save(&self, p: &Proposal) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE ms_design_proposal SET design_doc = $2, status = $3, review_comment = $4, \
             revision = $5 WHERE id = $1",
        )
        .bind(&p.id)
        .bind(&p.design_doc)
        .bind(p.status.as_str())
        .bind(&p.review_comment)
        .bind(p.revision as i32)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn list_by_requirement(&self, requirement_id: &str) -> Result<Vec<Proposal>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_design_proposal WHERE requirement_id = $1 ORDER BY seq"
        ))
        .bind(requirement_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_proposal).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_proposal_lifecycle_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        // 自建表(等价于迁移 0065_design_proposal.sql),不跑全量 migrate —— 测试自包含。
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ms_design_proposal (\
                seq BIGSERIAL, id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text, \
                requirement_id TEXT NOT NULL, title TEXT NOT NULL, design_doc TEXT, \
                status TEXT NOT NULL DEFAULT 'DRAFTING', review_comment TEXT, \
                revision INT NOT NULL DEFAULT 0)",
        )
        .execute(&pool)
        .await
        .expect("create table");
        sqlx::query("TRUNCATE ms_design_proposal").execute(&pool).await.expect("truncate");

        let repo = PgProposalRepository::new(pool.clone());
        let mut p = repo.create("req-1", "登录改造设计").await.expect("create");
        assert_eq!(p.status, ProposalStatus::Drafting);

        // 提交设计稿 → 待审 → 驳回(修订)→ 再提交 → 批准,逐步落库再读回。
        p.submit_design("## 方案 v1").expect("submit");
        repo.save(&p).await.expect("save1");
        p.request_changes("补充错误码").expect("reject");
        repo.save(&p).await.expect("save2");

        let got = repo.get(&p.id).await.expect("get").expect("some");
        assert_eq!(got.status, ProposalStatus::ChangesRequested);
        assert_eq!(got.revision, 1);
        assert_eq!(got.review_comment.as_deref(), Some("补充错误码"));

        let mut p = got;
        p.submit_design("## 方案 v2 含错误码").expect("submit2");
        p.approve().expect("approve");
        repo.save(&p).await.expect("save3");

        let final_p = repo.get(&p.id).await.expect("get2").expect("some");
        assert_eq!(final_p.status, ProposalStatus::Approved);
        assert_eq!(final_p.design_doc.as_deref(), Some("## 方案 v2 含错误码"));
        assert!(final_p.review_comment.is_none());
        assert_eq!(repo.list_by_requirement("req-1").await.expect("list").len(), 1);
    }
}
