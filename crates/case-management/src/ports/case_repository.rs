//! 功能用例仓储端口。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{FunctionalCase, NewFunctionalCase};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait CaseRepository: Send + Sync {
    /// 插入用例,返回含 id 的视图。
    async fn insert(&self, c: &NewFunctionalCase) -> Result<FunctionalCase, RepoError>;

    /// 列出项目下用例(排除软删除)。
    async fn list_by_project(&self, project_id: &str) -> Result<Vec<FunctionalCase>, RepoError>;

    /// 按 id 读用例(排除软删除)。
    async fn get(&self, id: &str) -> Result<Option<FunctionalCase>, RepoError>;
}
