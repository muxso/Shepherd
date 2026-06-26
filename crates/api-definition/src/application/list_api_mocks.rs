use std::sync::Arc;

use crate::domain::ApiMock;
use crate::ports::{ApiDefinitionRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ListApiMocksError {
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct ListApiMocksUseCase {
    repo: Arc<dyn ApiDefinitionRepository>,
}

impl ListApiMocksUseCase {
    pub fn new(repo: Arc<dyn ApiDefinitionRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        api_definition_id: &str,
    ) -> Result<Vec<ApiMock>, ListApiMocksError> {
        Ok(self.repo.list_mocks(api_definition_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiDefinitionRepository;
    use crate::application::{AddApiMockUseCase, CreateApiDefinitionUseCase};
    use crate::domain::ApiProtocol;

    #[tokio::test]
    async fn lists_mocks_for_definition() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let def = CreateApiDefinitionUseCase::new(repo.clone())
            .execute("p1", "x", ApiProtocol::Http, "GET", "/x", "u1")
            .await
            .expect("ok");
        let add = AddApiMockUseCase::new(repo.clone());
        add.execute(&def.id, "m1", serde_json::json!({}), 200, None, true, crate::application::ApiMockExtras::default(), "").await.expect("ok");
        add.execute(&def.id, "m2", serde_json::json!({}), 404, None, false, crate::application::ApiMockExtras::default(), "").await.expect("ok");

        let uc = ListApiMocksUseCase::new(repo);
        assert_eq!(uc.execute(&def.id).await.expect("ok").len(), 2);
    }
}
