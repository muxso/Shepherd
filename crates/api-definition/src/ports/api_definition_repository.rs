//! 接口定义仓储端口。覆盖接口定义 / 用例 / Mock 三聚合的读写。

use async_trait::async_trait;

use crate::domain::{ApiCase, ApiDefinition, ApiMock, NewApiCase, NewApiDefinition, NewApiMock};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait ApiDefinitionRepository: Send + Sync {
    /// 插入接口定义。
    async fn insert_definition(
        &self,
        d: &NewApiDefinition,
    ) -> Result<ApiDefinition, RepoError>;

    /// 按 id 读接口定义(排除软删除)。
    async fn get_definition(&self, id: &str) -> Result<Option<ApiDefinition>, RepoError>;

    /// 列出项目下的接口定义(排除软删除)。
    async fn list_definitions(
        &self,
        project_id: &str,
    ) -> Result<Vec<ApiDefinition>, RepoError>;

    /// 插入接口用例。
    async fn insert_case(&self, c: &NewApiCase) -> Result<ApiCase, RepoError>;

    /// 列出某接口定义下的用例。
    async fn list_cases(&self, api_definition_id: &str) -> Result<Vec<ApiCase>, RepoError>;

    /// 插入 Mock。
    async fn insert_mock(&self, m: &NewApiMock) -> Result<ApiMock, RepoError>;

    /// 列出某接口定义下的 Mock(排除软删除)。
    async fn list_mocks(&self, api_definition_id: &str) -> Result<Vec<ApiMock>, RepoError>;
}
