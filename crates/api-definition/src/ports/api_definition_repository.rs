//! 接口定义仓储端口。覆盖接口定义 / 用例 / Mock 三聚合的读写。

use async_trait::async_trait;

use crate::domain::{
    ApiCase, ApiDefinition, ApiModule, ApiMock, NewApiCase, NewApiDefinition, NewApiModule,
    NewApiMock,
};

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

    /// 统计项目下的用例总数(含独立用例,即 api_definition_id 为空者)。
    async fn count_cases_by_project(&self, project_id: &str) -> Result<u64, RepoError>;

    /// 分页列出项目下的用例(按插入顺序),offset/limit 由调用方算好。
    async fn list_cases_by_project(
        &self,
        project_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<ApiCase>, RepoError>;

    /// 插入 Mock。
    async fn insert_mock(&self, m: &NewApiMock) -> Result<ApiMock, RepoError>;

    /// 列出某接口定义下的 Mock(排除软删除)。
    async fn list_mocks(&self, api_definition_id: &str) -> Result<Vec<ApiMock>, RepoError>;

    // ---- 模块(文件夹)----

    /// 新建接口模块。
    async fn insert_module(&self, m: &NewApiModule) -> Result<ApiModule, RepoError>;

    /// 列出项目下的模块(排除软删除)。
    async fn list_modules(&self, project_id: &str) -> Result<Vec<ApiModule>, RepoError>;

    /// 重命名模块。
    async fn rename_module(&self, id: &str, name: &str) -> Result<(), RepoError>;

    /// 软删除模块,并把其下接口定义改为未归类(module_id = NULL)。
    async fn delete_module(&self, id: &str) -> Result<(), RepoError>;

    /// 把接口定义归入模块;module_id 为 None 表示移出到未归类。
    async fn set_definition_module(
        &self,
        definition_id: &str,
        module_id: Option<&str>,
    ) -> Result<(), RepoError>;

    // ---- 任务 ↔ 用例 关联 ----

    /// 关联一条用例到某任务(幂等)。
    async fn link_task_case(
        &self,
        decomposition_id: &str,
        task_id: &str,
        case_id: &str,
    ) -> Result<(), RepoError>;

    /// 解除关联。
    async fn unlink_task_case(
        &self,
        decomposition_id: &str,
        task_id: &str,
        case_id: &str,
    ) -> Result<(), RepoError>;

    /// 列出某任务关联的用例(join ms_api_case)。
    async fn list_cases_for_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<ApiCase>, RepoError>;
}
