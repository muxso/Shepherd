//! 任务拆分仓储端口。聚合(Decomposition + 任务 + 依赖边)整体读写。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::Decomposition;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait TaskRepository: Send + Sync {
    /// 为某需求版本新建一张空拆分图(分配 id)。唯一性(每版本一张)由调用方/DB 兜底。
    async fn create_decomposition(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Decomposition, RepoError>;

    /// 按需求版本查找已有拆分(用于唯一性判定)。
    async fn find_by_requirement_version(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Option<Decomposition>, RepoError>;

    /// 按 id 取回完整拆分(含全部任务与依赖边);不存在返回 None。
    async fn get(&self, id: &str) -> Result<Option<Decomposition>, RepoError>;

    /// 持久化聚合:追加新任务/依赖边并更新任务状态(幂等)。
    async fn save(&self, decomposition: &Decomposition) -> Result<(), RepoError>;
}
