use std::sync::Arc;

use crate::domain::{ApiDefinition, ApiDefinitionError, ApiProtocol, NewApiDefinition};
use crate::ports::{ApiDefinitionRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateApiDefinitionError {
    #[error(transparent)]
    Validation(#[from] ApiDefinitionError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateApiDefinitionUseCase {
    repo: Arc<dyn ApiDefinitionRepository>,
}

impl CreateApiDefinitionUseCase {
    pub fn new(repo: Arc<dyn ApiDefinitionRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        project_id: &str,
        name: &str,
        protocol: ApiProtocol,
        method: &str,
        path: &str,
        created_by: &str,
    ) -> Result<ApiDefinition, CreateApiDefinitionError> {
        let new_def = NewApiDefinition::new(project_id, name, protocol, method, path)?
            .with_created_by(created_by);
        Ok(self.repo.insert_definition(&new_def).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiDefinitionRepository;

    #[tokio::test]
    async fn creates_definition() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = CreateApiDefinitionUseCase::new(repo);
        let d =
            uc.execute("p1", "登录", ApiProtocol::Http, "get", "/login", "u1").await.expect("ok");
        assert_eq!(d.method, "GET");
        assert_eq!(d.protocol, ApiProtocol::Http);
    }

    #[tokio::test]
    async fn rejects_invalid_method() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = CreateApiDefinitionUseCase::new(repo);
        let err = uc.execute("p1", "x", ApiProtocol::Http, "FETCH", "/", "u1").await.unwrap_err();
        assert_eq!(
            err,
            CreateApiDefinitionError::Validation(ApiDefinitionError::UnknownMethod("FETCH".into()))
        );
    }
}
