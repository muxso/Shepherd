use std::sync::Arc;

use crate::domain::{ApiDefinition, ApiDefinitionError, ApiProtocol, NewApiDefinition};
use crate::ports::{ApiDefinitionRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UpdateApiDefinitionError {
    #[error(transparent)]
    Validation(#[from] ApiDefinitionError),
    #[error("api definition not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct UpdateApiDefinitionUseCase {
    repo: Arc<dyn ApiDefinitionRepository>,
}

impl UpdateApiDefinitionUseCase {
    pub fn new(repo: Arc<dyn ApiDefinitionRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        id: &str,
        name: &str,
        protocol: ApiProtocol,
        method: &str,
        path: &str,
    ) -> Result<ApiDefinition, UpdateApiDefinitionError> {
        let validated = NewApiDefinition::new("_", name, protocol, method, path)?;
        self.repo
            .update_definition(id, &validated.name, protocol.as_str(), &validated.method, path)
            .await?
            .ok_or(UpdateApiDefinitionError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiDefinitionRepository;
    use crate::application::CreateApiDefinitionUseCase;

    async fn seed(repo: Arc<InMemoryApiDefinitionRepository>) -> String {
        let uc = CreateApiDefinitionUseCase::new(repo);
        uc.execute("p1", "登录", ApiProtocol::Http, "get", "/login", "u1").await.expect("seed").id
    }

    #[tokio::test]
    async fn updates_basic_fields() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let id = seed(repo.clone()).await;
        let uc = UpdateApiDefinitionUseCase::new(repo);
        let d =
            uc.execute(&id, " 登录v2 ", ApiProtocol::Http, "post", "/v2/login").await.expect("ok");
        assert_eq!(d.name, "登录v2");
        assert_eq!(d.method, "POST");
        assert_eq!(d.path, "/v2/login");
        assert_eq!(d.project_id, "p1");
    }

    #[tokio::test]
    async fn rejects_invalid_method() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let id = seed(repo.clone()).await;
        let uc = UpdateApiDefinitionUseCase::new(repo);
        let err = uc.execute(&id, "x", ApiProtocol::Http, "FETCH", "/").await.unwrap_err();
        assert_eq!(
            err,
            UpdateApiDefinitionError::Validation(ApiDefinitionError::UnknownMethod("FETCH".into()))
        );
    }

    #[tokio::test]
    async fn missing_definition_not_found() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = UpdateApiDefinitionUseCase::new(repo);
        let err = uc.execute("ghost", "x", ApiProtocol::Http, "GET", "/").await.unwrap_err();
        assert_eq!(err, UpdateApiDefinitionError::NotFound);
    }
}
