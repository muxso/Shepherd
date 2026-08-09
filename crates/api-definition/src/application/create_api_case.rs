use std::sync::Arc;

use crate::domain::{ApiCase, ApiDefinitionError, NewApiCase};
use crate::ports::{ApiDefinitionRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateApiCaseError {
    #[error(transparent)]
    Validation(#[from] ApiDefinitionError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateApiCaseUseCase {
    repo: Arc<dyn ApiDefinitionRepository>,
}

impl CreateApiCaseUseCase {
    pub fn new(repo: Arc<dyn ApiDefinitionRepository>) -> Self {
        Self { repo }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        project_id: &str,
        api_definition_id: Option<&str>,
        name: &str,
        method: &str,
        url: &str,
        body: Option<String>,
        assertions: serde_json::Value,
        processors: serde_json::Value,
    ) -> Result<ApiCase, CreateApiCaseError> {
        let def_id = api_definition_id.unwrap_or_default();
        let new_case = NewApiCase::new(def_id, project_id, name, method, url, body, assertions)?
            .with_processors(processors);
        Ok(self.repo.insert_case(&new_case).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiDefinitionRepository;

    #[tokio::test]
    async fn creates_standalone_case_with_empty_definition() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = CreateApiCaseUseCase::new(repo);
        let c = uc
            .execute(
                "p1",
                None,
                "case",
                "post",
                "/login",
                None,
                serde_json::json!([]),
                serde_json::json!([]),
            )
            .await
            .expect("ok");
        assert_eq!(c.project_id, "p1");
        assert_eq!(c.api_definition_id, "");
        assert_eq!(c.method, "POST");
    }

    #[tokio::test]
    async fn creates_case_under_definition() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = CreateApiCaseUseCase::new(repo);
        let c = uc
            .execute(
                "p1",
                Some("def-1"),
                "case",
                "GET",
                "/x",
                None,
                serde_json::json!([]),
                serde_json::json!([]),
            )
            .await
            .expect("ok");
        assert_eq!(c.api_definition_id, "def-1");
    }

    #[tokio::test]
    async fn rejects_bad_assertions() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = CreateApiCaseUseCase::new(repo);
        let err = uc
            .execute(
                "p1",
                None,
                "case",
                "GET",
                "/x",
                None,
                serde_json::json!({}),
                serde_json::json!([]),
            )
            .await
            .unwrap_err();
        assert_eq!(err, CreateApiCaseError::Validation(ApiDefinitionError::BadAssertions));
    }
}
