use std::sync::Arc;

use crate::domain::PassRule;
use crate::ports::{RepoError, ReviewMetaUpdate, ReviewRepository};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UpdateReviewMetaError {
    #[error("invalid review meta: {0}")]
    Validation(&'static str),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

/// Edits the review header (name / description / tags / module / schedule) and pass setting.
#[derive(Clone)]
pub struct UpdateReviewMetaUseCase {
    repo: Arc<dyn ReviewRepository>,
}

impl UpdateReviewMetaUseCase {
    pub fn new(repo: Arc<dyn ReviewRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        review_id: &str,
        mut update: ReviewMetaUpdate,
    ) -> Result<(), UpdateReviewMetaError> {
        update.meta.name = update.meta.name.trim().to_string();
        if update.meta.name.is_empty() {
            return Err(UpdateReviewMetaError::Validation("name is required"));
        }
        // Same normalization as create: empty rule falls back to SINGLE, count is at least 1.
        let rule = update.pass_rule.trim();
        update.pass_rule = if rule.is_empty() {
            PassRule::Single.as_str().to_string()
        } else {
            PassRule::parse(rule)
                .ok_or(UpdateReviewMetaError::Validation("unknown pass rule"))?
                .as_str()
                .to_string()
        };
        update.reviewer_count = update.reviewer_count.max(1);
        self.repo.update_review_meta(review_id, &update).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryReviewRepository;
    use crate::ports::{NewReview, ReviewMeta};

    fn meta(name: &str) -> ReviewMeta {
        ReviewMeta { name: name.to_string(), ..ReviewMeta::default() }
    }

    fn update(name: &str, rule: &str, count: usize) -> ReviewMetaUpdate {
        ReviewMetaUpdate { pass_rule: rule.to_string(), reviewer_count: count, meta: meta(name) }
    }

    async fn seeded_repo() -> (Arc<InMemoryReviewRepository>, String) {
        let repo = Arc::new(InMemoryReviewRepository::new());
        let id = repo
            .create_review(&NewReview {
                project_id: "p1".into(),
                pass_rule: "SINGLE".into(),
                reviewer_count: 1,
                case_ids: vec!["c1".into()],
                created_by: "admin".into(),
                meta: meta("v1 评审"),
            })
            .await
            .expect("create");
        (repo, id)
    }

    #[tokio::test]
    async fn updates_meta_and_setting() {
        let (repo, id) = seeded_repo().await;
        let uc = UpdateReviewMetaUseCase::new(repo.clone());
        let mut u = update("v2 评审", "MULTIPLE", 3);
        u.meta.description = "desc".into();
        u.meta.tags = vec!["t1".into()];
        u.meta.module_id = Some("m1".into());
        u.meta.start_at = Some("2026-07-01T00:00:00Z".into());
        u.meta.end_at = Some("2026-07-31T00:00:00Z".into());
        uc.execute(&id, u).await.expect("update");

        let list = repo.list_reviews("p1").await.expect("list");
        let s = &list[0];
        assert_eq!(s.meta.name, "v2 评审");
        assert_eq!(s.meta.description, "desc");
        assert_eq!(s.meta.tags, vec!["t1".to_string()]);
        assert_eq!(s.meta.module_id.as_deref(), Some("m1"));
        assert_eq!(s.pass_rule, "MULTIPLE");
        assert_eq!(s.reviewer_count, 3);
    }

    #[tokio::test]
    async fn blank_name_is_rejected() {
        let (repo, id) = seeded_repo().await;
        let uc = UpdateReviewMetaUseCase::new(repo);
        let err = uc.execute(&id, update("   ", "SINGLE", 1)).await.expect_err("must fail");
        assert_eq!(err, UpdateReviewMetaError::Validation("name is required"));
    }

    #[tokio::test]
    async fn unknown_pass_rule_is_rejected() {
        let (repo, id) = seeded_repo().await;
        let uc = UpdateReviewMetaUseCase::new(repo);
        let err = uc.execute(&id, update("n", "WAT", 1)).await.expect_err("must fail");
        assert_eq!(err, UpdateReviewMetaError::Validation("unknown pass rule"));
    }

    #[tokio::test]
    async fn empty_rule_defaults_to_single_and_count_is_clamped() {
        let (repo, id) = seeded_repo().await;
        let uc = UpdateReviewMetaUseCase::new(repo.clone());
        uc.execute(&id, update("n", "", 0)).await.expect("update");
        let s = &repo.list_reviews("p1").await.expect("list")[0];
        assert_eq!(s.pass_rule, "SINGLE");
        assert_eq!(s.reviewer_count, 1);
    }

    #[tokio::test]
    async fn missing_review_is_not_found() {
        let repo = Arc::new(InMemoryReviewRepository::new());
        let uc = UpdateReviewMetaUseCase::new(repo);
        let err = uc.execute("nope", update("n", "SINGLE", 1)).await.expect_err("must fail");
        assert_eq!(err, UpdateReviewMetaError::Repo(RepoError::NotFound));
    }
}
