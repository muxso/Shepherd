use async_trait::async_trait;

use crate::domain::Follow;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait FollowStore: Send + Sync {
    async fn follow(&self, f: &Follow) -> Result<bool, RepoError>;

    async fn unfollow(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
        user_id: &str,
    ) -> Result<bool, RepoError>;

    async fn followers(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Vec<String>, RepoError>;

    async fn following_ids(
        &self,
        project_id: &str,
        user_id: &str,
        entity_type: Option<&str>,
    ) -> Result<Vec<String>, RepoError>;
}
