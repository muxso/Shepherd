//! 项目仓储端口。async trait,内存与未来的 PG 实现都满足它。
//!
//! 注意命名带 "active":唯一性判定与列表都**只看未软删除**的项目,
//! 把"软删除语义"固化进端口契约,而不是寄望每个调用方自己记得加 `WHERE deleted = false`。

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
    /// 在组织内按名查找**未删除**的项目,用于唯一性判定。
    async fn find_active_by_name(
        &self,
        organization_id: &str,
        name: &str,
    ) -> Result<Option<Project>, RepoError>;

    async fn insert(&self, new_project: &NewProject) -> Result<Project, RepoError>;

    /// 组织内**未删除**项目总数(分页用)。
    async fn count_active(&self, organization_id: &str) -> Result<u64, RepoError>;

    /// 组织内**未删除**项目分页切片(按插入顺序)。
    async fn list_active(
        &self,
        organization_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<Project>, RepoError>;
}
