use async_trait::async_trait;

use crate::domain::{
    ApiCase, ApiDefinition, ApiDefinitionChange, ApiMock, ApiModule, ApiView, NewApiCase,
    NewApiDefinition, NewApiMock, NewApiModule, NewApiView,
};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone)]
pub struct ProjectMockRow {
    pub mock: ApiMock,
    pub method: String,
    pub path: String,
    pub protocol: String,
    pub definition_name: String,
    pub operator: String,
    pub updated_at: String,
}

#[async_trait]
pub trait ApiDefinitionRepository: Send + Sync {
    async fn insert_definition(&self, d: &NewApiDefinition) -> Result<ApiDefinition, RepoError>;

    async fn get_definition(&self, id: &str) -> Result<Option<ApiDefinition>, RepoError>;

    async fn update_definition(
        &self,
        id: &str,
        name: &str,
        protocol: &str,
        method: &str,
        path: &str,
    ) -> Result<Option<ApiDefinition>, RepoError>;

    async fn update_definition_spec(&self, id: &str, spec: &str) -> Result<(), RepoError>;

    async fn update_definition_status(&self, id: &str, status: &str) -> Result<(), RepoError>;

    async fn delete_definition(&self, id: &str) -> Result<(), RepoError>;

    async fn list_definitions(&self, project_id: &str) -> Result<Vec<ApiDefinition>, RepoError>;

    async fn record_definition_change(
        &self,
        definition_id: &str,
        action: &str,
        detail: &str,
        actor: &str,
    ) -> Result<(), RepoError>;

    async fn list_definition_changes(
        &self,
        definition_id: &str,
    ) -> Result<Vec<ApiDefinitionChange>, RepoError>;

    async fn insert_case(&self, c: &NewApiCase) -> Result<ApiCase, RepoError>;

    async fn update_case(&self, id: &str, c: &NewApiCase) -> Result<bool, RepoError>;

    async fn delete_case(&self, id: &str) -> Result<bool, RepoError>;

    async fn update_mock(&self, mock_id: &str, m: &NewApiMock) -> Result<bool, RepoError>;

    async fn delete_mock(&self, mock_id: &str) -> Result<bool, RepoError>;

    async fn list_cases(&self, api_definition_id: &str) -> Result<Vec<ApiCase>, RepoError>;

    async fn count_cases_by_project(&self, project_id: &str) -> Result<u64, RepoError>;

    async fn list_cases_by_project(
        &self,
        project_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<ApiCase>, RepoError>;

    async fn insert_mock(&self, m: &NewApiMock) -> Result<ApiMock, RepoError>;

    async fn list_mocks(&self, api_definition_id: &str) -> Result<Vec<ApiMock>, RepoError>;

    async fn list_mocks_by_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectMockRow>, RepoError>;

    async fn insert_module(&self, m: &NewApiModule) -> Result<ApiModule, RepoError>;

    async fn list_modules(&self, project_id: &str) -> Result<Vec<ApiModule>, RepoError>;

    async fn rename_module(&self, id: &str, name: &str) -> Result<(), RepoError>;

    async fn delete_module(&self, id: &str) -> Result<(), RepoError>;

    async fn set_definition_module(
        &self,
        definition_id: &str,
        module_id: Option<&str>,
    ) -> Result<(), RepoError>;

    async fn insert_view(&self, v: &NewApiView) -> Result<ApiView, RepoError>;

    async fn list_views(&self, project_id: &str, user_id: &str) -> Result<Vec<ApiView>, RepoError>;

    async fn delete_view(&self, id: &str, user_id: &str) -> Result<(), RepoError>;

    async fn link_task_case(
        &self,
        decomposition_id: &str,
        task_id: &str,
        case_id: &str,
    ) -> Result<(), RepoError>;

    async fn unlink_task_case(
        &self,
        decomposition_id: &str,
        task_id: &str,
        case_id: &str,
    ) -> Result<(), RepoError>;

    async fn list_cases_for_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<ApiCase>, RepoError>;
}
