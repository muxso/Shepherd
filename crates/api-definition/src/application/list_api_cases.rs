//! 用例:列出某接口定义下的用例。

use std::sync::Arc;

use crate::domain::ApiCase;
use crate::ports::{ApiDefinitionRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ListApiCasesError {
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct ListApiCasesUseCase {
    repo: Arc<dyn ApiDefinitionRepository>,
}

impl ListApiCasesUseCase {
    pub fn new(repo: Arc<dyn ApiDefinitionRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        api_definition_id: &str,
    ) -> Result<Vec<ApiCase>, ListApiCasesError> {
        Ok(self.repo.list_cases(api_definition_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiDefinitionRepository;
    use crate::application::{AddApiCaseUseCase, CreateApiDefinitionUseCase};
    use crate::domain::ApiProtocol;

    #[tokio::test]
    async fn lists_cases_for_definition() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let def = CreateApiDefinitionUseCase::new(repo.clone())
            .execute("p1", "x", ApiProtocol::Http, "GET", "/x")
            .await
            .expect("ok");
        let add = AddApiCaseUseCase::new(repo.clone());
        add.execute(&def.id, "c1", "GET", "/x", None, serde_json::json!([])).await.expect("ok");
        add.execute(&def.id, "c2", "GET", "/x", None, serde_json::json!([])).await.expect("ok");

        let uc = ListApiCasesUseCase::new(repo);
        assert_eq!(uc.execute(&def.id).await.expect("ok").len(), 2);
    }
}
