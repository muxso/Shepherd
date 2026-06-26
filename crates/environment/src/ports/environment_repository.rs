use async_trait::async_trait;

use crate::domain::{Environment, NewEnvironment};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait EnvironmentRepository: Send + Sync {
    async fn insert(&self, e: &NewEnvironment) -> Result<Environment, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<Environment>, RepoError>;

    async fn list_by_project(&self, project_id: &str) -> Result<Vec<Environment>, RepoError>;

    async fn update(&self, id: &str, e: &NewEnvironment) -> Result<Option<Environment>, RepoError>;

    async fn delete(&self, id: &str) -> Result<bool, RepoError>;
}
