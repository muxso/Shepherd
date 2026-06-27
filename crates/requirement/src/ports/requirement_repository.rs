//! 端口契约:`*_active` 方法一律只看未软删除的需求。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{NewRequirement, Requirement, StatusCounts};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait RequirementRepository: Send + Sync {
    async fn find_active_by_title(
        &self,
        project_id: &str,
        title: &str,
    ) -> Result<Option<Requirement>, RepoError>;

    async fn insert(&self, new: &NewRequirement) -> Result<Requirement, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<Requirement>, RepoError>;

    async fn count_active(&self, project_id: &str) -> Result<u64, RepoError>;

    async fn list_active(
        &self,
        project_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<Requirement>, RepoError>;

    /// 版本不可变:save 只追加尚未落库的版本,不改写已存在的。
    async fn save(&self, requirement: &Requirement) -> Result<(), RepoError>;

    /// 手工排序:按给定顺序为这些需求写入显式秩(1..N);未列出的保持原秩。
    async fn set_order(&self, project_id: &str, ordered_ids: &[String]) -> Result<(), RepoError>;

    /// 项目内未删除需求按状态聚合(仪表盘)。
    async fn status_counts(&self, project_id: &str) -> Result<StatusCounts, RepoError>;
}
