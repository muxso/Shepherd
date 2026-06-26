//! `_active` 方法一律只看未软删除的项目(软删除语义固化进端口契约)。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{NewProject, Project};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn find_active_by_name(
        &self,
        organization_id: &str,
        name: &str,
    ) -> Result<Option<Project>, RepoError>;

    async fn insert(&self, new_project: &NewProject) -> Result<Project, RepoError>;

    async fn count_active(&self, organization_id: &str) -> Result<u64, RepoError>;

    async fn list_active(
        &self,
        organization_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<Project>, RepoError>;
}
