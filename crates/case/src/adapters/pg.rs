//! PostgreSQL 实现的 `ReviewRepository`。
//!
//! 状态/规则在 DB 里以规范字符串存储,借领域的 `as_str`/`parse` 双向映射;
//! 历史按 `seq` 升序读出,正好满足聚合算法"时间升序"的输入约定。

use async_trait::async_trait;
use crate::domain::{PassRule, ReviewRecord, ReviewSetting, ReviewStatus};
use crate::ports::{RepoError, ReviewRepository};
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PgReviewRepository {
    pool: PgPool,
}

impl PgReviewRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn parse_status(s: &str) -> Result<ReviewStatus, RepoError> {
    ReviewStatus::parse(s).ok_or_else(|| RepoError::Backend(format!("bad review status: {s}")))
}

#[async_trait]
impl ReviewRepository for PgReviewRepository {
    async fn review_setting(&self, review_id: &str) -> Result<ReviewSetting, RepoError> {
        let row = sqlx::query("SELECT pass_rule, reviewer_count FROM ms_case_review WHERE id = $1")
            .bind(review_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?
            .ok_or(RepoError::NotFound)?;

        let rule_raw: String = row.try_get("pass_rule").map_err(map_err)?;
        let rule = PassRule::parse(&rule_raw)
            .ok_or_else(|| RepoError::Backend(format!("bad pass_rule: {rule_raw}")))?;
        let count: i32 = row.try_get("reviewer_count").map_err(map_err)?;
        Ok(ReviewSetting { rule, reviewer_count: count.max(0) as usize })
    }

    async fn history_of(
        &self,
        review_id: &str,
        case_id: &str,
    ) -> Result<Vec<ReviewRecord>, RepoError> {
        let rows = sqlx::query(
            "SELECT reviewer_id, status FROM ms_case_review_history \
             WHERE review_id = $1 AND case_id = $2 ORDER BY seq",
        )
        .bind(review_id)
        .bind(case_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;

        rows.iter()
            .map(|r| {
                let reviewer_id: String = r.try_get("reviewer_id").map_err(map_err)?;
                let status_raw: String = r.try_get("status").map_err(map_err)?;
                Ok(ReviewRecord { reviewer_id, status: parse_status(&status_raw)? })
            })
            .collect()
    }

    async fn append_history(
        &self,
        review_id: &str,
        case_id: &str,
        record: &ReviewRecord,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO ms_case_review_history (review_id, case_id, reviewer_id, status) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(review_id)
        .bind(case_id)
        .bind(&record.reviewer_id)
        .bind(record.status.as_str())
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn set_case_status(
        &self,
        review_id: &str,
        case_id: &str,
        status: ReviewStatus,
    ) -> Result<(), RepoError> {
        // 回写:存在则更新(UPSERT)
        sqlx::query(
            "INSERT INTO ms_case_review_status (review_id, case_id, status) VALUES ($1, $2, $3) \
             ON CONFLICT (review_id, case_id) DO UPDATE SET status = EXCLUDED.status",
        )
        .bind(review_id)
        .bind(case_id)
        .bind(status.as_str())
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
    async fn pg_history_ordering_and_status_upsert() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::raw_sql(
            "TRUNCATE ms_case_review, ms_case_review_history, ms_case_review_status RESTART IDENTITY",
        )
        .execute(&pool)
        .await
        .expect("truncate");

        // 评审配置:会签,2 人
        sqlx::query("INSERT INTO ms_case_review (id, pass_rule, reviewer_count) VALUES ('rev1','MULTIPLE',2)")
            .execute(&pool)
            .await
            .expect("seed setting");

        let repo = PgReviewRepository::new(pool.clone());

        // setting 往返
        let setting = repo.review_setting("rev1").await.expect("setting");
        assert_eq!(setting.rule, PassRule::Multiple);
        assert_eq!(setting.reviewer_count, 2);
        assert_eq!(repo.review_setting("nope").await, Err(RepoError::NotFound));

        // 追加历史并验证时间升序
        repo.append_history("rev1", "c1", &ReviewRecord { reviewer_id: "u1".into(), status: ReviewStatus::UnPass })
            .await
            .expect("h1");
        repo.append_history("rev1", "c1", &ReviewRecord { reviewer_id: "u1".into(), status: ReviewStatus::Pass })
            .await
            .expect("h2");
        let hist = repo.history_of("rev1", "c1").await.expect("hist");
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].status, ReviewStatus::UnPass); // 先
        assert_eq!(hist[1].status, ReviewStatus::Pass); // 后

        // 状态 UPSERT:两次,第二次覆盖
        repo.set_case_status("rev1", "c1", ReviewStatus::UnderReviewed).await.expect("s1");
        repo.set_case_status("rev1", "c1", ReviewStatus::Pass).await.expect("s2");
        let row = sqlx::query("SELECT status FROM ms_case_review_status WHERE review_id='rev1' AND case_id='c1'")
            .fetch_one(&pool)
            .await
            .expect("status row");
        let s: String = row.try_get("status").expect("status");
        assert_eq!(s, "PASS");
    }
}
