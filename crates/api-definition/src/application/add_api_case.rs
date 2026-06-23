//! 用例:为某接口定义新增用例。接口定义不存在则 NotFound;project_id 取自接口定义。

use std::sync::Arc;

use crate::domain::{ApiCase, ApiDefinitionError, NewApiCase};
use crate::ports::{ApiDefinitionRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AddApiCaseError {
    #[error(transparent)]
    Validation(#[from] ApiDefinitionError),
    #[error("api definition not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct AddApiCaseUseCase {
    repo: Arc<dyn ApiDefinitionRepository>,
}

impl AddApiCaseUseCase {
    pub fn new(repo: Arc<dyn ApiDefinitionRepository>) -> Self {
        Self { repo }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        api_definition_id: &str,
        name: &str,
        method: &str,
        url: &str,
        body: Option<String>,
        assertions: serde_json::Value,
        processors: serde_json::Value,
        meta: ApiCaseMeta,
    ) -> Result<ApiCase, AddApiCaseError> {
        // 用例的归属项目以接口定义为准,不由调用方指定,避免错配。
        let def = self
            .repo
            .get_definition(api_definition_id)
            .await?
            .ok_or(AddApiCaseError::NotFound)?;
        let new_case = NewApiCase::new(
            api_definition_id,
            &def.project_id,
            name,
            method,
            url,
            body,
            assertions,
        )?
        .with_processors(processors)
        .with_meta(&meta.priority, &meta.status, meta.tags)
        .with_headers(meta.headers)
        .with_request(meta.query_params, meta.rest_params, meta.auth);
        Ok(self.repo.insert_case(&new_case).await?)
    }
}

/// 用例元信息(优先级 / 状态 / 标签 / 请求头 / Query / REST / 认证),收拢成一个参数,避免 execute 形参爆炸。
#[derive(Debug, Clone, Default)]
pub struct ApiCaseMeta {
    pub priority: String,
    pub status: String,
    pub tags: serde_json::Value,
    pub headers: serde_json::Value,
    pub query_params: serde_json::Value,
    pub rest_params: serde_json::Value,
    pub auth: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiDefinitionRepository;
    use crate::application::CreateApiDefinitionUseCase;
    use crate::domain::ApiProtocol;

    #[tokio::test]
    async fn adds_case_and_inherits_project() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let def = CreateApiDefinitionUseCase::new(repo.clone())
            .execute("p1", "登录", ApiProtocol::Http, "POST", "/login", "u1")
            .await
            .expect("ok");
        let uc = AddApiCaseUseCase::new(repo);
        let c = uc
            .execute(&def.id, "用例", "POST", "/login", None, serde_json::json!([]), serde_json::json!([]), ApiCaseMeta::default())
            .await
            .expect("ok");
        assert_eq!(c.project_id, "p1");
        assert_eq!(c.api_definition_id, def.id);
    }

    #[tokio::test]
    async fn missing_definition_is_not_found() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = AddApiCaseUseCase::new(repo);
        let err = uc
            .execute("ghost", "用例", "GET", "/x", None, serde_json::json!([]), serde_json::json!([]), ApiCaseMeta::default())
            .await
            .unwrap_err();
        assert_eq!(err, AddApiCaseError::NotFound);
    }

    #[tokio::test]
    async fn rejects_bad_assertions() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let def = CreateApiDefinitionUseCase::new(repo.clone())
            .execute("p1", "x", ApiProtocol::Http, "GET", "/x", "u1")
            .await
            .expect("ok");
        let uc = AddApiCaseUseCase::new(repo);
        let err = uc
            .execute(&def.id, "用例", "GET", "/x", None, serde_json::json!({}), serde_json::json!([]), ApiCaseMeta::default())
            .await
            .unwrap_err();
        assert_eq!(err, AddApiCaseError::Validation(ApiDefinitionError::BadAssertions));
    }
}
