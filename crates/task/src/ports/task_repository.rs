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
    async fn create_decomposition(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Decomposition, RepoError>;

    async fn find_by_requirement_version(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Option<Decomposition>, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<Decomposition>, RepoError>;

    /// Whole-graph write for add-task/build only; do NOT use for status changes (concurrent siblings lose updates) — use [`Self::save_task_status`].
    async fn save(&self, decomposition: &Decomposition) -> Result<(), RepoError>;

    /// Row-level atomic status update, avoiding the whole-graph lost-update.
    async fn save_task_status(
        &self,
        decomposition_id: &str,
        task_id: &str,
        status: TaskStatus,
    ) -> Result<(), RepoError>;
}
