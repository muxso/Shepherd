//! 用例:导入接口定义(OpenAPI 3.x / Swagger 2.0)。
//!
//! 解析文档为一批接口(纯函数 [`parse_openapi`]),逐条建为接口定义。整体「尽力而为」:
//! 单条建失败不阻断其余,返回成功建出的定义 + 跳过的条数,便于前端反馈导入结果。

use std::sync::Arc;

use crate::domain::{parse_openapi, ApiDefinition, ApiDefinitionError, ApiProtocol, NewApiDefinition};
use crate::ports::{ApiDefinitionRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImportError {
    /// 文档无法解析为 OpenAPI/Swagger。
    #[error(transparent)]
    Parse(#[from] ApiDefinitionError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

/// 导入结果:建出的定义 + 跳过条数(校验失败但不阻断整体的)。
#[derive(Debug)]
pub struct ImportOutcome {
    pub created: Vec<ApiDefinition>,
    pub skipped: usize,
}

#[derive(Clone)]
pub struct ImportApiDefinitionsUseCase {
    repo: Arc<dyn ApiDefinitionRepository>,
}

impl ImportApiDefinitionsUseCase {
    pub fn new(repo: Arc<dyn ApiDefinitionRepository>) -> Self {
        Self { repo }
    }

    /// `doc` 为解析后的 JSON 文档。先解析(失败整体报错),再逐条建定义。
    pub async fn execute(
        &self,
        project_id: &str,
        doc: &serde_json::Value,
    ) -> Result<ImportOutcome, ImportError> {
        let apis = parse_openapi(doc)?;
        let mut created = Vec::new();
        let mut skipped = 0usize;
        for api in apis {
            match NewApiDefinition::new(project_id, &api.name, ApiProtocol::Http, &api.method, &api.path) {
                Ok(new_def) => created.push(self.repo.insert_definition(&new_def).await?),
                Err(_) => skipped += 1, // 单条校验失败:跳过,不阻断整体
            }
        }
        Ok(ImportOutcome { created, skipped })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiDefinitionRepository;
    use crate::ports::ApiDefinitionRepository;
    use serde_json::json;

    #[tokio::test]
    async fn imports_openapi_creates_one_definition_per_operation() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = ImportApiDefinitionsUseCase::new(repo.clone());
        let doc = json!({
            "openapi": "3.0.0",
            "paths": {
                "/login": { "post": { "summary": "登录" } },
                "/users": { "get": { "operationId": "listUsers" } }
            }
        });
        let out = uc.execute("p1", &doc).await.expect("imported");
        assert_eq!(out.created.len(), 2);
        assert_eq!(out.skipped, 0);
        assert_eq!(repo.list_definitions("p1").await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn unparseable_doc_errors() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = ImportApiDefinitionsUseCase::new(repo);
        let err = uc.execute("p1", &json!({"foo": 1})).await.unwrap_err();
        assert!(matches!(err, ImportError::Parse(ApiDefinitionError::BadImport(_))));
    }
}
