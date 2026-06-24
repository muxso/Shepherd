//! 接口定义仓储端口。覆盖接口定义 / 用例 / Mock 三聚合的读写。

use async_trait::async_trait;

use crate::domain::{
    ApiCase, ApiDefinition, ApiDefinitionChange, ApiModule, ApiMock, ApiView, NewApiCase,
    NewApiDefinition, NewApiModule, NewApiMock, NewApiView,
};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// 项目级 Mock 读模型:Mock + 其所属接口定义的 方法/路径/协议/名称。
#[derive(Debug, Clone)]
pub struct ProjectMockRow {
    pub mock: ApiMock,
    pub method: String,
    pub path: String,
    pub protocol: String,
    pub definition_name: String,
    /// 操作人(created_by)+ 更新时间(文本);0057 后回填。
    pub operator: String,
    pub updated_at: String,
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

    /// 更新接口定义的基础字段(名称/协议/方法/路径)。返回更新后的实体;
    /// `None` 表示未命中(不存在或已软删)。project_id/状态/模块/spec 不变。
    async fn update_definition(
        &self,
        id: &str,
        name: &str,
        protocol: &str,
        method: &str,
        path: &str,
    ) -> Result<Option<ApiDefinition>, RepoError>;

    /// 更新接口定义的请求/响应规格(spec,不透明 JSON 文本)。
    async fn update_definition_spec(&self, id: &str, spec: &str) -> Result<(), RepoError>;

    /// 更新接口定义状态(DRAFT/DEBUGGING/COMPLETED/DEPRECATED)。
    async fn update_definition_status(&self, id: &str, status: &str) -> Result<(), RepoError>;

    /// 删除接口定义:软删定义本身,连带软删其 Mock、硬删其用例(用例无软删列)。
    async fn delete_definition(&self, id: &str) -> Result<(), RepoError>;

    /// 列出项目下的接口定义(排除软删除)。
    async fn list_definitions(
        &self,
        project_id: &str,
    ) -> Result<Vec<ApiDefinition>, RepoError>;

    /// 追加一条接口定义变更历史(审计)。
    async fn record_definition_change(
        &self,
        definition_id: &str,
        action: &str,
        detail: &str,
        actor: &str,
    ) -> Result<(), RepoError>;

    /// 列出某接口定义的变更历史(按时间倒序,最新在前)。
    async fn list_definition_changes(
        &self,
        definition_id: &str,
    ) -> Result<Vec<ApiDefinitionChange>, RepoError>;

    /// 插入接口用例。
    async fn insert_case(&self, c: &NewApiCase) -> Result<ApiCase, RepoError>;

    /// 更新接口用例的可变字段(不动 api_definition_id/project_id)。返回是否命中。
    async fn update_case(&self, id: &str, c: &NewApiCase) -> Result<bool, RepoError>;

    /// 删除接口用例(软删/硬删由实现决定)。返回是否命中。
    async fn delete_case(&self, id: &str) -> Result<bool, RepoError>;

    /// 更新 Mock 的可变字段(不动 api_definition_id/created_by;回填 updated_at)。返回是否命中。
    async fn update_mock(&self, mock_id: &str, m: &NewApiMock) -> Result<bool, RepoError>;

    /// 软删 Mock(按 mock id)。返回是否命中。
    async fn delete_mock(&self, mock_id: &str) -> Result<bool, RepoError>;

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

    /// 列出项目下的全部 Mock(JOIN 接口定义带出 方法/路径/协议;排除软删除)。供「MOCK 视图」。
    async fn list_mocks_by_project(&self, project_id: &str) -> Result<Vec<ProjectMockRow>, RepoError>;

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

    // ---- 列表视图(保存筛选/列设置/页大小)----

    /// 保存一个视图。
    async fn insert_view(&self, v: &NewApiView) -> Result<ApiView, RepoError>;

    /// 列出项目下该用户可见的视图(本人创建 + 共享),按创建时间倒序。
    async fn list_views(&self, project_id: &str, user_id: &str) -> Result<Vec<ApiView>, RepoError>;

    /// 删除视图(仅本人创建可删)。
    async fn delete_view(&self, id: &str, user_id: &str) -> Result<(), RepoError>;

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
