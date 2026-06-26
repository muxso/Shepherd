use std::sync::Arc;

use crate::ports::{CommentRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeleteCommentError {
    #[error("comment not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct DeleteCommentUseCase {
    repo: Arc<dyn CommentRepository>,
}

impl DeleteCommentUseCase {
    pub fn new(repo: Arc<dyn CommentRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, id: &str) -> Result<(), DeleteCommentError> {
        if self.repo.get(id).await?.is_none() {
            return Err(DeleteCommentError::NotFound);
        }
        self.repo.soft_delete(id).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryCommentRepository;
    use crate::application::{AddCommentUseCase, ListCommentsUseCase};

    #[tokio::test]
    async fn deletes_existing_comment() {
        let repo = Arc::new(InMemoryCommentRepository::new());
        let c = AddCommentUseCase::new(repo.clone())
            .execute("BUG", "b1", "x", "a")
            .await
            .expect("ok");
        DeleteCommentUseCase::new(repo.clone()).execute(&c.id).await.expect("deleted");
        let list = ListCommentsUseCase::new(repo).execute("BUG", "b1").await.expect("ok");
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn deleting_missing_comment_is_not_found() {
        let repo = Arc::new(InMemoryCommentRepository::new());
        let err = DeleteCommentUseCase::new(repo).execute("ghost").await.unwrap_err();
        assert_eq!(err, DeleteCommentError::NotFound);
    }
}
