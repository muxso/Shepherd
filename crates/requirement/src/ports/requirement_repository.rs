//! 需求仓储端口。async trait,内存与 PG 实现都满足它。
//!
//! 与 project 一样把"软删除语义"固化进端口契约:`find_active_by_title` / `count_active` /
//! `list_active` 一律**只看未软删除**的需求,而非寄望调用方记得加 `WHERE deleted = false`。
//! 聚合(含全部版本快照)整体读写:`get` 加载完整版本历史,`save` 落库元信息 + 追加新版本。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{NewRequirement, Requirement};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait RequirementRepository: Send + Sync {
    /// 项目内按标题查找**未删除**的需求,用于唯一性判定。
    async fn find_active_by_title(
        &self,
        project_id: &str,
        title: &str,
    ) -> Result<Option<Requirement>, RepoError>;

    /// 插入新需求(建聚合 + 版本 1)。
    async fn insert(&self, new: &NewRequirement) -> Result<Requirement, RepoError>;

    /// 按 id 取回完整聚合(含全部版本快照);不存在返回 None。
    async fn get(&self, id: &str) -> Result<Option<Requirement>, RepoError>;

    /// 项目内**未删除**需求总数(分页用)。
    async fn count_active(&self, project_id: &str) -> Result<u64, RepoError>;

    /// 项目内**未删除**需求分页切片(按创建顺序)。
    async fn list_active(
        &self,
        project_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<Requirement>, RepoError>;

    /// 持久化聚合变更:更新元信息(标题/状态/基线/删除标记)并**追加**尚未落库的版本快照
    /// (版本不可变,已存在的版本不改写)。
    async fn save(&self, requirement: &Requirement) -> Result<(), RepoError>;
}
