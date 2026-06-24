//! 用例:更新某条 Mock 的可变字段。Mock 不存在(或已软删)则 NotFound。

use std::sync::Arc;

use crate::application::ApiMockExtras;
use crate::domain::{ApiDefinitionError, NewApiMock};
use crate::ports::{ApiDefinitionRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UpdateApiMockError {
    #[error(transparent)]
    Validation(#[from] ApiDefinitionError),
    #[error("api mock not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct UpdateApiMockUseCase {
    repo: Arc<dyn ApiDefinitionRepository>,
}

impl UpdateApiMockUseCase {
    pub fn new(repo: Arc<dyn ApiDefinitionRepository>) -> Self {
        Self { repo }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        mock_id: &str,
        name: &str,
        match_rule: serde_json::Value,
        response_status: i32,
        response_body: Option<String>,
        enabled: bool,
        extras: ApiMockExtras,
    ) -> Result<(), UpdateApiMockError> {
        // api_definition_id 在更新中不变;用占位空串构造(NewApiMock 仅校验 name/status/match_rule)。
        let updated = NewApiMock::new("_", name, match_rule, response_status, response_body, enabled)?
            .with_extras(
                extras.tags,
                extras.response_headers,
                extras.response_delay_ms,
                extras.follow_definition,
            );
        if self.repo.update_mock(mock_id, &updated).await? {
            Ok(())
        } else {
            Err(UpdateApiMockError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiDefinitionRepository;
    use crate::application::{AddApiMockUseCase, CreateApiDefinitionUseCase};
    use crate::domain::ApiProtocol;

    #[tokio::test]
    async fn updates_mock() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let def = CreateApiDefinitionUseCase::new(repo.clone())
            .execute("p1", "x", ApiProtocol::Http, "GET", "/x", "u1")
            .await
            .expect("ok");
        let m = AddApiMockUseCase::new(repo.clone())
            .execute(&def.id, "old", serde_json::json!({}), 200, None, true, ApiMockExtras::default(), "u1")
            .await
            .expect("ok");

        let uc = UpdateApiMockUseCase::new(repo.clone());
        uc.execute(&m.id, "new", serde_json::json!({"path": "/y"}), 404, Some("x".into()), false, ApiMockExtras::default())
            .await
            .expect("ok");

        let mocks = repo.list_mocks(&def.id).await.expect("ok");
        assert_eq!(mocks.len(), 1);
        assert_eq!(mocks[0].name, "new");
        assert_eq!(mocks[0].response_status, 404);
        assert!(!mocks[0].enabled);
        assert_eq!(mocks[0].api_definition_id, def.id);
    }

    #[tokio::test]
    async fn missing_mock_is_not_found() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = UpdateApiMockUseCase::new(repo);
        let err = uc
            .execute("ghost", "n", serde_json::json!({}), 200, None, true, ApiMockExtras::default())
            .await
            .unwrap_err();
        assert_eq!(err, UpdateApiMockError::NotFound);
    }

    #[tokio::test]
    async fn rejects_status_out_of_range() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let def = CreateApiDefinitionUseCase::new(repo.clone())
            .execute("p1", "x", ApiProtocol::Http, "GET", "/x", "u1")
            .await
            .expect("ok");
        let m = AddApiMockUseCase::new(repo.clone())
            .execute(&def.id, "old", serde_json::json!({}), 200, None, true, ApiMockExtras::default(), "u1")
            .await
            .expect("ok");
        let uc = UpdateApiMockUseCase::new(repo);
        let err = uc
            .execute(&m.id, "old", serde_json::json!({}), 700, None, true, ApiMockExtras::default())
            .await
            .unwrap_err();
        assert_eq!(err, UpdateApiMockError::Validation(ApiDefinitionError::BadResponseStatus(700)));
    }
}
