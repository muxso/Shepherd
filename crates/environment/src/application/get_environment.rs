use std::sync::Arc;

use crate::domain::Environment;
use crate::ports::{EnvironmentRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GetEnvironmentError {
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct GetEnvironmentUseCase {
    repo: Arc<dyn EnvironmentRepository>,
}

impl GetEnvironmentUseCase {
    pub fn new(repo: Arc<dyn EnvironmentRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, id: &str) -> Result<Option<Environment>, GetEnvironmentError> {
        Ok(self.repo.get(id).await?)
    }
}
