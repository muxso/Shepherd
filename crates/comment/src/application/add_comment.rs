use std::sync::Arc;

use crate::domain::{Comment, CommentError, NewComment};
use crate::ports::{CommentRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AddCommentError {
    #[error(transparent)]
    Validation(#[from] CommentError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct AddCommentUseCase {
    repo: Arc<dyn CommentRepository>,
}

impl AddCommentUseCase {
    pub fn new(repo: Arc<dyn CommentRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        target_type: &str,
        target_id: &str,
        content: &str,
        author: &str,
    ) -> Result<Comment, AddCommentError> {
        let new_comment = NewComment::new(target_type, target_id, content, author)?;
        Ok(self.repo.insert(&new_comment).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryCommentRepository;

    #[tokio::test]
    async fn adds_valid_comment() {
        let uc = AddCommentUseCase::new(Arc::new(InMemoryCommentRepository::new()));
        let c = uc.execute("BUG", "b1", "复现步骤如下", "admin").await.expect("ok");
        assert_eq!(c.target_id, "b1");
        assert_eq!(c.content, "复现步骤如下");
        assert_eq!(c.author, "admin");
    }

    #[tokio::test]
    async fn rejects_blank_content() {
        let uc = AddCommentUseCase::new(Arc::new(InMemoryCommentRepository::new()));
        let err = uc.execute("BUG", "b1", "   ", "admin").await.unwrap_err();
        assert_eq!(err, AddCommentError::Validation(CommentError::EmptyContent));
    }
}
