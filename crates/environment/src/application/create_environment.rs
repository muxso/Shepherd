use std::sync::Arc;

use crate::application::EnvironmentInput;
use crate::domain::{Environment, EnvironmentError, NewEnvironment};
use crate::ports::{EnvironmentRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateEnvironmentError {
    #[error(transparent)]
    Validation(#[from] EnvironmentError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateEnvironmentUseCase {
    repo: Arc<dyn EnvironmentRepository>,
}

impl CreateEnvironmentUseCase {
    pub fn new(repo: Arc<dyn EnvironmentRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        input: EnvironmentInput,
    ) -> Result<Environment, CreateEnvironmentError> {
        let new_env = NewEnvironment::new(
            &input.project_id,
            &input.name,
            &input.base_url,
            input.headers,
            input.variables,
            input.enabled,
        )?;
        Ok(self.repo.insert(&new_env).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryEnvironmentRepository;
    use std::collections::BTreeMap;

    fn input() -> EnvironmentInput {
        EnvironmentInput {
            project_id: "p1".into(),
            name: "local".into(),
            base_url: "http://localhost:8088".into(),
            headers: vec![("Authorization".into(), "Bearer x".into())],
            variables: BTreeMap::new(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn creates_environment() {
        let repo = Arc::new(InMemoryEnvironmentRepository::new());
        let uc = CreateEnvironmentUseCase::new(repo);
        let e = uc.execute(input()).await.expect("ok");
        assert_eq!(e.name, "local");
        assert_eq!(e.headers.len(), 1);
        assert!(e.enabled);
    }

    #[tokio::test]
    async fn rejects_blank_name() {
        let repo = Arc::new(InMemoryEnvironmentRepository::new());
        let uc = CreateEnvironmentUseCase::new(repo);
        let mut bad = input();
        bad.name = "  ".into();
        assert_eq!(
            uc.execute(bad).await.unwrap_err(),
            CreateEnvironmentError::Validation(EnvironmentError::EmptyName)
        );
    }
}
