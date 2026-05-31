//! 交付仓储端口。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{DeliveryAttempt, ExecutorKind};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait DeliveryRepository: Send + Sync {
    /// 新建一次交付尝试(分配 id,初始 Dispatched)。
    async fn create(
        &self,
        decomposition_id: &str,
        task_id: &str,
        executor: ExecutorKind,
    ) -> Result<DeliveryAttempt, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<DeliveryAttempt>, RepoError>;

    /// 某任务的全部交付尝试(按创建顺序)。
    async fn list_by_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<DeliveryAttempt>, RepoError>;

    async fn save(&self, attempt: &DeliveryAttempt) -> Result<(), RepoError>;
}
