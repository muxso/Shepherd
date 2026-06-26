use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{
    AttemptStatus, DeliveryAttempt, ExecutionEvent, ExecutorKind, NewExecutionEvent,
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, Default)]
pub struct TaskListFilter {
    pub status: Option<AttemptStatus>,
    pub executor: Option<ExecutorKind>,
    pub active_only: bool,
    pub query: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRow {
    pub attempt: DeliveryAttempt,
    pub title: Option<String>,
    pub description: Option<String>,
    pub module: Option<String>,
    pub created_at: i64,
    pub event_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPage {
    pub items: Vec<TaskRow>,
    pub total: i64,
}

#[async_trait]
pub trait DeliveryRepository: Send + Sync {
    async fn create(
        &self,
        decomposition_id: &str,
        task_id: &str,
        executor: ExecutorKind,
    ) -> Result<DeliveryAttempt, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<DeliveryAttempt>, RepoError>;

    async fn list_by_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<DeliveryAttempt>, RepoError>;

    async fn save(&self, attempt: &DeliveryAttempt) -> Result<(), RepoError>;

    async fn append_event(
        &self,
        attempt_id: &str,
        event: &NewExecutionEvent,
    ) -> Result<ExecutionEvent, RepoError>;

    async fn list_events(&self, attempt_id: &str) -> Result<Vec<ExecutionEvent>, RepoError>;

    async fn list_tasks(&self, filter: &TaskListFilter) -> Result<TaskPage, RepoError>;

    async fn delete(&self, id: &str) -> Result<bool, RepoError>;
}
