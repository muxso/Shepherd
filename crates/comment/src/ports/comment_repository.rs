use async_trait::async_trait;

use crate::domain::{Comment, NewComment};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait CommentRepository: Send + Sync {
    async fn insert(&self, new_comment: &NewComment) -> Result<Comment, RepoError>;

    async fn list(&self, target_type: &str, target_id: &str) -> Result<Vec<Comment>, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<Comment>, RepoError>;

    async fn soft_delete(&self, id: &str) -> Result<(), RepoError>;
}
