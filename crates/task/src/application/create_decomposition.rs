//! 用例:为一个需求版本开启任务拆分(每版本至多一张拆分图)。

use std::sync::Arc;

use thiserror::Error;

use crate::domain::Decomposition;
use crate::ports::{RepoError, TaskRepository};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateDecompositionError {
    #[error("requirement id must not be empty")]
    EmptyRequirement,
    #[error("decomposition already exists for this requirement version")]
    AlreadyExists,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateDecompositionUseCase {
    repo: Arc<dyn TaskRepository>,
}

impl CreateDecompositionUseCase {
    pub fn new(repo: Arc<dyn TaskRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Decomposition, CreateDecompositionError> {
        let requirement_id = requirement_id.trim();
        if requirement_id.is_empty() {
            return Err(CreateDecompositionError::EmptyRequirement);
        }
        if self
            .repo
            .find_by_requirement_version(requirement_id, requirement_version)
            .await?
            .is_some()
        {
            return Err(CreateDecompositionError::AlreadyExists);
        }
        Ok(self.repo.create_decomposition(requirement_id, requirement_version).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryTaskRepository;

    fn uc() -> CreateDecompositionUseCase {
        CreateDecompositionUseCase::new(Arc::new(InMemoryTaskRepository::new()))
    }

    #[tokio::test]
    async fn creates_empty_decomposition() {
        let d = uc().execute("req1", 1).await.expect("ok");
        assert_eq!(d.requirement_id, "req1");
        assert_eq!(d.requirement_version, 1);
        assert!(d.tasks.is_empty());
    }

    #[tokio::test]
    async fn rejects_blank_requirement() {
        assert_eq!(uc().execute("  ", 1).await.unwrap_err(), CreateDecompositionError::EmptyRequirement);
    }

    #[tokio::test]
    async fn one_decomposition_per_requirement_version() {
        let uc = uc();
        uc.execute("req1", 1).await.expect("first");
        assert_eq!(uc.execute("req1", 1).await.unwrap_err(), CreateDecompositionError::AlreadyExists);
        // 不同版本可以各有一张
        assert!(uc.execute("req1", 2).await.is_ok());
    }
}
