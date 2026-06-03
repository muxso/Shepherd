//! 用例:为某接口定义新增 Mock。接口定义不存在则 NotFound。

use std::sync::Arc;

use crate::domain::{ApiDefinitionError, ApiMock, NewApiMock};
use crate::ports::{ApiDefinitionRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AddApiMockError {
    #[error(transparent)]
    Validation(#[from] ApiDefinitionError),
    #[error("api definition not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct AddApiMockUseCase {
    repo: Arc<dyn ApiDefinitionRepository>,
}

impl AddApiMockUseCase {
    pub fn new(repo: Arc<dyn ApiDefinitionRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        api_definition_id: &str,
        name: &str,
        match_rule: serde_json::Value,
        response_status: i32,
        response_body: Option<String>,
        enabled: bool,
    ) -> Result<ApiMock, AddApiMockError> {
        // 必须挂在一个真实存在的接口定义上。
        if self.repo.get_definition(api_definition_id).await?.is_none() {
            return Err(AddApiMockError::NotFound);
        }
        let new_mock = NewApiMock::new(
            api_definition_id,
            name,
            match_rule,
            response_status,
            response_body,
            enabled,
        )?;
        Ok(self.repo.insert_mock(&new_mock).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiDefinitionRepository;
    use crate::application::CreateApiDefinitionUseCase;
    use crate::domain::ApiProtocol;

    #[tokio::test]
    async fn adds_mock() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let def = CreateApiDefinitionUseCase::new(repo.clone())
            .execute("p1", "x", ApiProtocol::Http, "GET", "/x")
            .await
            .expect("ok");
        let uc = AddApiMockUseCase::new(repo);
        let m = uc
            .execute(&def.id, "挡板", serde_json::json!({}), 200, None, true)
            .await
            .expect("ok");
        assert_eq!(m.response_status, 200);
        assert_eq!(m.api_definition_id, def.id);
    }

    #[tokio::test]
    async fn missing_definition_is_not_found() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = AddApiMockUseCase::new(repo);
        let err = uc
            .execute("ghost", "挡板", serde_json::json!({}), 200, None, true)
            .await
            .unwrap_err();
        assert_eq!(err, AddApiMockError::NotFound);
    }

    #[tokio::test]
    async fn rejects_status_out_of_range() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let def = CreateApiDefinitionUseCase::new(repo.clone())
            .execute("p1", "x", ApiProtocol::Http, "GET", "/x")
            .await
            .expect("ok");
        let uc = AddApiMockUseCase::new(repo);
        let err = uc
            .execute(&def.id, "挡板", serde_json::json!({}), 700, None, true)
            .await
            .unwrap_err();
        assert_eq!(err, AddApiMockError::Validation(ApiDefinitionError::BadResponseStatus(700)));
    }
}
