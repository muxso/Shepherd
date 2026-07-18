use crate::domain::{PassRule, ReviewRecord, ReviewSetting, ReviewStatus};
use crate::ports::{
    NewReview, RepoError, ReviewCaseStatus, ReviewDetail, ReviewMeta, ReviewMetaUpdate,
    ReviewRepository, ReviewSummary,
};
use async_trait::async_trait;
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

    async fn create_review(&self, req: &NewReview) -> Result<String, RepoError> {
        let rule = PassRule::parse(&req.pass_rule).unwrap_or(PassRule::Single);
        let tags =
            serde_json::to_string(&req.meta.tags).map_err(|e| RepoError::Backend(e.to_string()))?;
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        let id: String = sqlx::query(
            "INSERT INTO ms_case_review \
               (id, pass_rule, reviewer_count, project_id, case_ids, \
                name, description, tags, module_id, start_at, end_at, created_by) \
             VALUES (gen_random_uuid()::text, $1, $2, $3, $4, $5, $6, $7::jsonb, $8, \
                NULLIF($9, '')::timestamptz, NULLIF($10, '')::timestamptz, $11) RETURNING id",
        )
        .bind(rule.as_str())
        .bind(req.reviewer_count as i32)
        .bind(&req.project_id)
        .bind(req.case_ids.clone())
        .bind(&req.meta.name)
        .bind(&req.meta.description)
        .bind(&tags)
        .bind(&req.meta.module_id)
        .bind(req.meta.start_at.clone().unwrap_or_default())
        .bind(req.meta.end_at.clone().unwrap_or_default())
        .bind(&req.created_by)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_err)?
        .try_get("id")
        .map_err(map_err)?;
        for c in &req.case_ids {
            sqlx::query(
                "INSERT INTO ms_case_review_status (review_id, case_id, status) VALUES ($1, $2, 'UN_REVIEWED') \
                 ON CONFLICT (review_id, case_id) DO NOTHING",
            )
            .bind(&id)
            .bind(c)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(id)
    }

    async fn update_review_meta(
        &self,
        review_id: &str,
        update: &ReviewMetaUpdate,
    ) -> Result<(), RepoError> {
        let tags = serde_json::to_string(&update.meta.tags)
            .map_err(|e| RepoError::Backend(e.to_string()))?;
        let res = sqlx::query(
            "UPDATE ms_case_review SET \
                name = $2, description = $3, tags = $4::jsonb, module_id = $5, \
                start_at = NULLIF($6, '')::timestamptz, end_at = NULLIF($7, '')::timestamptz, \
                pass_rule = $8, reviewer_count = $9 \
             WHERE id = $1 AND NOT deleted",
        )
        .bind(review_id)
        .bind(&update.meta.name)
        .bind(&update.meta.description)
        .bind(&tags)
        .bind(&update.meta.module_id)
        .bind(update.meta.start_at.clone().unwrap_or_default())
        .bind(update.meta.end_at.clone().unwrap_or_default())
        .bind(&update.pass_rule)
        .bind(update.reviewer_count as i32)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        if res.rows_affected() == 0 {
            return Err(RepoError::NotFound);
        }
        Ok(())
    }

    async fn list_reviews(&self, project_id: &str) -> Result<Vec<ReviewSummary>, RepoError> {
        let rows = sqlx::query(
            "SELECT r.id, r.pass_rule, r.reviewer_count, r.created_at::text AS created_at, \
                    r.name, r.description, r.tags::text AS tags, r.module_id, \
                    r.start_at::text AS start_at, r.end_at::text AS end_at, r.created_by, \
                    coalesce(array_length(r.case_ids, 1), 0) AS total, \
                    (SELECT count(*) FROM ms_case_review_status s WHERE s.review_id = r.id AND s.status = 'PASS') AS passed, \
                    (SELECT coalesce(array_agg(DISTINCT h.reviewer_id), '{}') \
                       FROM ms_case_review_history h WHERE h.review_id = r.id) AS reviewers \
             FROM ms_case_review r WHERE r.project_id = $1 AND NOT r.deleted ORDER BY r.created_at DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter()
            .map(|r| {
                let count: i32 = r.try_get("reviewer_count").map_err(map_err)?;
                let total: i32 = r.try_get("total").map_err(map_err)?;
                let passed: i64 = r.try_get("passed").map_err(map_err)?;
                let total = total.max(0) as usize;
                let passed = passed.max(0) as usize;
                let status = if total > 0 && passed >= total { "COMPLETED" } else { "IN_PROGRESS" };
                let tags_raw: String = r.try_get("tags").map_err(map_err)?;
                let tags: Vec<String> = serde_json::from_str(&tags_raw).unwrap_or_default();
                Ok(ReviewSummary {
                    id: r.try_get("id").map_err(map_err)?,
                    pass_rule: r.try_get("pass_rule").map_err(map_err)?,
                    reviewer_count: count.max(0) as usize,
                    total,
                    passed,
                    created_at: r.try_get("created_at").map_err(map_err)?,
                    status: status.to_string(),
                    created_by: r.try_get("created_by").map_err(map_err)?,
                    reviewers: r.try_get("reviewers").map_err(map_err)?,
                    meta: ReviewMeta {
                        name: r.try_get("name").map_err(map_err)?,
                        description: r.try_get("description").map_err(map_err)?,
                        tags,
                        module_id: r.try_get("module_id").map_err(map_err)?,
                        start_at: r.try_get("start_at").map_err(map_err)?,
                        end_at: r.try_get("end_at").map_err(map_err)?,
                    },
                })
            })
            .collect()
    }

    async fn get_review(&self, review_id: &str) -> Result<ReviewDetail, RepoError> {
        let head = sqlx::query("SELECT pass_rule, reviewer_count, case_ids FROM ms_case_review WHERE id = $1 AND NOT deleted")
            .bind(review_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?
            .ok_or(RepoError::NotFound)?;
        let pass_rule: String = head.try_get("pass_rule").map_err(map_err)?;
        let count: i32 = head.try_get("reviewer_count").map_err(map_err)?;
        let case_ids: Vec<String> = head.try_get("case_ids").map_err(map_err)?;
        let srows =
            sqlx::query("SELECT case_id, status FROM ms_case_review_status WHERE review_id = $1")
                .bind(review_id)
                .fetch_all(&self.pool)
                .await
                .map_err(map_err)?;
        let mut status_of = std::collections::HashMap::new();
        for r in &srows {
            let cid: String = r.try_get("case_id").map_err(map_err)?;
            let st: String = r.try_get("status").map_err(map_err)?;
            status_of.insert(cid, st);
        }
        let cases = case_ids
            .into_iter()
            .map(|cid| {
                let status =
                    status_of.get(&cid).cloned().unwrap_or_else(|| "UN_REVIEWED".to_string());
                ReviewCaseStatus { case_id: cid, status }
            })
            .collect();
        Ok(ReviewDetail {
            id: review_id.to_string(),
            pass_rule,
            reviewer_count: count.max(0) as usize,
            cases,
        })
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

        sqlx::query("INSERT INTO ms_case_review (id, pass_rule, reviewer_count) VALUES ('rev1','MULTIPLE',2)")
            .execute(&pool)
            .await
            .expect("seed setting");

        let repo = PgReviewRepository::new(pool.clone());

        let setting = repo.review_setting("rev1").await.expect("setting");
        assert_eq!(setting.rule, PassRule::Multiple);
        assert_eq!(setting.reviewer_count, 2);
        assert_eq!(repo.review_setting("nope").await, Err(RepoError::NotFound));

        repo.append_history(
            "rev1",
            "c1",
            &ReviewRecord { reviewer_id: "u1".into(), status: ReviewStatus::UnPass },
        )
        .await
        .expect("h1");
        repo.append_history(
            "rev1",
            "c1",
            &ReviewRecord { reviewer_id: "u1".into(), status: ReviewStatus::Pass },
        )
        .await
        .expect("h2");
        let hist = repo.history_of("rev1", "c1").await.expect("hist");
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].status, ReviewStatus::UnPass);
        assert_eq!(hist[1].status, ReviewStatus::Pass);

        repo.set_case_status("rev1", "c1", ReviewStatus::UnderReviewed).await.expect("s1");
        repo.set_case_status("rev1", "c1", ReviewStatus::Pass).await.expect("s2");
        let row = sqlx::query(
            "SELECT status FROM ms_case_review_status WHERE review_id='rev1' AND case_id='c1'",
        )
        .fetch_one(&pool)
        .await
        .expect("status row");
        let s: String = row.try_get("status").expect("status");
        assert_eq!(s, "PASS");
    }
}
