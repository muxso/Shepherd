//! 任务拆分仓储端口。聚合(Decomposition + 任务 + 依赖边)整体读写。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{Decomposition, TaskStatus};

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
    /// 注意:整图写,仅用于「加任务/建图」。**状态推进禁用此法**——并发推进同图兄弟任务时
    /// 各自回写整张快照会互相覆盖(丢更新)。单任务状态变更请走 [`save_task_status`]。
    async fn save(&self, decomposition: &Decomposition) -> Result<(), RepoError>;

    /// 原子更新**单个**任务的状态(行级写)。避免并发推进兄弟任务时整图回写互相覆盖。
    async fn save_task_status(
        &self,
        decomposition_id: &str,
        task_id: &str,
        status: TaskStatus,
    ) -> Result<(), RepoError>;
}
