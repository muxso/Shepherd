use std::sync::Arc;

use crate::domain::{aggregate_status, ReviewError, ReviewRecord, ReviewStatus, Verdict};
use crate::ports::{RepoError, ReviewRepository};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SubmitReviewError {
    #[error(transparent)]
    Validation(#[from] ReviewError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct SubmitReviewUseCase {
    repo: Arc<dyn ReviewRepository>,
}

impl SubmitReviewUseCase {
    pub fn new(repo: Arc<dyn ReviewRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        review_id: &str,
        case_id: &str,
        verdict: Verdict,
    ) -> Result<ReviewStatus, SubmitReviewError> {
        let setting = self.repo.review_setting(review_id).await?;

        let record = ReviewRecord { reviewer_id: verdict.reviewer_id, status: verdict.status };
        self.repo.append_history(review_id, case_id, &record).await?;

        let history = self.repo.history_of(review_id, case_id).await?;

        let status = aggregate_status(setting, &history);

        self.repo.set_case_status(review_id, case_id, status).await?;
        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryReviewRepository;
    use crate::domain::{PassRule, ReviewSetting, ReviewStatus::*};

    fn uc(repo: &InMemoryReviewRepository) -> SubmitReviewUseCase {
        SubmitReviewUseCase::new(Arc::new(repo.clone()))
    }

    #[tokio::test]
    async fn single_pass_sets_case_pass_and_persists() {
        let repo = InMemoryReviewRepository::new();
        repo.set_setting("rev1", ReviewSetting { rule: PassRule::Single, reviewer_count: 1 });
        let uc = uc(&repo);

        let v = Verdict::new("u1", Pass, None).expect("valid");
        let status = uc.execute("rev1", "c1", v).await.expect("ok");
        assert_eq!(status, Pass);
        assert_eq!(repo.case_status("rev1", "c1"), Some(Pass));
    }

    #[tokio::test]
    async fn multiple_first_reviewer_pass_is_under_review() {
        let repo = InMemoryReviewRepository::new();
        repo.set_setting("rev1", ReviewSetting { rule: PassRule::Multiple, reviewer_count: 2 });
        let uc = uc(&repo);

        let status =
            uc.execute("rev1", "c1", Verdict::new("u1", Pass, None).expect("v")).await.expect("ok");
        assert_eq!(status, UnderReviewed);
    }

    #[tokio::test]
    async fn multiple_both_reviewers_pass_is_pass() {
        let repo = InMemoryReviewRepository::new();
        repo.set_setting("rev1", ReviewSetting { rule: PassRule::Multiple, reviewer_count: 2 });
        let uc = uc(&repo);

        uc.execute("rev1", "c1", Verdict::new("u1", Pass, None).expect("v")).await.expect("ok");
        let status =
            uc.execute("rev1", "c1", Verdict::new("u2", Pass, None).expect("v")).await.expect("ok");
        assert_eq!(status, Pass);
    }

    #[tokio::test]
    async fn un_pass_without_content_is_rejected_before_persisting() {
        let repo = InMemoryReviewRepository::new();
        repo.set_setting("rev1", ReviewSetting { rule: PassRule::Single, reviewer_count: 1 });

        let bad = Verdict::new("u1", UnPass, None);
        assert_eq!(bad, Err(ReviewError::ContentRequiredForUnPass));
        assert!(repo.history_of_sync("rev1", "c1").is_empty());
    }

    #[tokio::test]
    async fn un_pass_with_content_sets_case_unpass() {
        let repo = InMemoryReviewRepository::new();
        repo.set_setting("rev1", ReviewSetting { rule: PassRule::Multiple, reviewer_count: 2 });
        let uc = uc(&repo);

        uc.execute("rev1", "c1", Verdict::new("u1", Pass, None).expect("v")).await.expect("ok");
        let status = uc
            .execute("rev1", "c1", Verdict::new("u2", UnPass, Some("步骤缺失")).expect("v"))
            .await
            .expect("ok");
        assert_eq!(status, UnPass);
    }
}
