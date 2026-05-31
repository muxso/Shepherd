//! 缺陷仓储端口。提供缺陷读写 + 项目的状态流图。

use async_trait::async_trait;

use crate::domain::{Bug, NewBug, StatusFlowGraph};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait BugRepository: Send + Sync {
    /// 项目配置的缺陷状态流图。
    async fn status_flow(&self, project_id: &str) -> Result<StatusFlowGraph, RepoError>;

    /// 以给定初始状态插入缺陷。
    async fn insert(&self, new_bug: &NewBug, initial_status: &str) -> Result<Bug, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<Bug>, RepoError>;

    async fn set_status(&self, id: &str, status: &str) -> Result<(), RepoError>;
}
