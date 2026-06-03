//! 用例:删除环境(软删)。

use std::sync::Arc;

use crate::ports::{EnvironmentRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeleteEnvironmentError {
    #[error(transparent)]
    Repo(#[from] RepoError),
    #[error("environment not found")]
    NotFound,
}

#[derive(Clone)]
pub struct DeleteEnvironmentUseCase {
    repo: Arc<dyn EnvironmentRepository>,
}

impl DeleteEnvironmentUseCase {
    pub fn new(repo: Arc<dyn EnvironmentRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, id: &str) -> Result<(), DeleteEnvironmentError> {
        if self.repo.delete(id).await? {
            Ok(())
        } else {
            Err(DeleteEnvironmentError::NotFound)
        }
    }
}
