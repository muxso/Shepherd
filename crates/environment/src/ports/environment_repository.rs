//! 环境仓储端口。

use async_trait::async_trait;

use crate::domain::{Environment, NewEnvironment};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait EnvironmentRepository: Send + Sync {
    /// 插入环境。
    async fn insert(&self, e: &NewEnvironment) -> Result<Environment, RepoError>;

    /// 按 id 读环境(排除软删除)。
    async fn get(&self, id: &str) -> Result<Option<Environment>, RepoError>;

    /// 列出项目下的环境(排除软删除)。
    async fn list_by_project(&self, project_id: &str) -> Result<Vec<Environment>, RepoError>;

    /// 更新环境(project_id 不可变,以 id 定位)。不存在返回 `None`。
    async fn update(&self, id: &str, e: &NewEnvironment) -> Result<Option<Environment>, RepoError>;

    /// 软删除环境。返回是否命中。
    async fn delete(&self, id: &str) -> Result<bool, RepoError>;
}
