use std::sync::Arc;

use crate::ports::{RepoError, ReviewRepository};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeleteReviewError {
    #[error(transparent)]
    Repo(#[from] RepoError),
}

/// Soft-deletes a review; the list/get views stop returning it.
#[derive(Clone)]
pub struct DeleteReviewUseCase {
    repo: Arc<dyn ReviewRepository>,
}

impl DeleteReviewUseCase {
    pub fn new(repo: Arc<dyn ReviewRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, review_id: &str) -> Result<(), DeleteReviewError> {
        self.repo.delete_review(review_id).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryReviewRepository;
    use crate::ports::{NewReview, ReviewMeta};

    async fn seeded_repo() -> (Arc<InMemoryReviewRepository>, String) {
        let repo = Arc::new(InMemoryReviewRepository::new());
        let id = repo
            .create_review(&NewReview {
                project_id: "p1".into(),
                pass_rule: "SINGLE".into(),
                reviewer_count: 1,
                case_ids: vec!["c1".into()],
                created_by: "admin".into(),
                meta: ReviewMeta { name: "v1 review".into(), ..ReviewMeta::default() },
            })
            .await
            .expect("create");
        (repo, id)
    }

    #[tokio::test]
    async fn delete_hides_review_from_list_and_get() {
        let (repo, id) = seeded_repo().await;
        let uc = DeleteReviewUseCase::new(repo.clone());
        uc.execute(&id).await.expect("delete");
        assert!(repo.list_reviews("p1").await.expect("list").is_empty());
        assert_eq!(repo.get_review(&id).await, Err(RepoError::NotFound));
    }

    #[tokio::test]
    async fn delete_missing_review_is_not_found() {
        let repo = Arc::new(InMemoryReviewRepository::new());
        let uc = DeleteReviewUseCase::new(repo);
        let err = uc.execute("nope").await.expect_err("must fail");
        assert_eq!(err, DeleteReviewError::Repo(RepoError::NotFound));
    }

    #[tokio::test]
    async fn delete_twice_is_not_found() {
        let (repo, id) = seeded_repo().await;
        let uc = DeleteReviewUseCase::new(repo);
        uc.execute(&id).await.expect("delete");
        let err = uc.execute(&id).await.expect_err("must fail");
        assert_eq!(err, DeleteReviewError::Repo(RepoError::NotFound));
    }
}
