//! 用例:更新接口定义的基础字段(名称/协议/方法/路径)。
//!
//! 校验复用 `NewApiDefinition`(名称非空、HTTP 方法白名单并规整为大写);
//! project_id/状态/模块/spec 不在本用例职责内,保持不变。

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
        // 复用建模校验:占位 project 仅为通过非空校验(本用例不改 project_id)。
        // 校验通过后取其规整后的 name/method(HTTP 大写)。
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
        uc.execute("p1", "登录", ApiProtocol::Http, "get", "/login", "u1")
            .await
            .expect("seed")
            .id
    }

    #[tokio::test]
    async fn updates_basic_fields() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let id = seed(repo.clone()).await;
        let uc = UpdateApiDefinitionUseCase::new(repo);
        let d = uc
            .execute(&id, " 登录v2 ", ApiProtocol::Http, "post", "/v2/login")
            .await
            .expect("ok");
        assert_eq!(d.name, "登录v2");
        assert_eq!(d.method, "POST");
        assert_eq!(d.path, "/v2/login");
        // project_id 不变。
        assert_eq!(d.project_id, "p1");
    }

    #[tokio::test]
    async fn rejects_invalid_method() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let id = seed(repo.clone()).await;
        let uc = UpdateApiDefinitionUseCase::new(repo);
        let err = uc
            .execute(&id, "x", ApiProtocol::Http, "FETCH", "/")
            .await
            .unwrap_err();
        assert_eq!(
            err,
            UpdateApiDefinitionError::Validation(ApiDefinitionError::UnknownMethod("FETCH".into()))
        );
    }

    #[tokio::test]
    async fn missing_definition_not_found() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = UpdateApiDefinitionUseCase::new(repo);
        let err = uc
            .execute("ghost", "x", ApiProtocol::Http, "GET", "/")
            .await
            .unwrap_err();
        assert_eq!(err, UpdateApiDefinitionError::NotFound);
    }
}
