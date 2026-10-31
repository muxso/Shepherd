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

/// AI/human split of verified tasks under one requirement.
///
/// Definition: a VERIFIED task with a DELIVERED delivery record counts as AI-delivered;
/// VERIFIED without one counts as human-delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollabRequirementRow {
    pub requirement_id: String,
    pub title: String,
    pub ai_tasks: i64,
    pub human_tasks: i64,
    pub ai_points: i64,
    pub human_points: i64,
    /// Delivery quality at attempt level (not limited to verified tasks):
    /// total / succeeded / failed attempts.
    pub ai_attempts: i64,
    pub ai_delivered: i64,
    pub ai_failed: i64,
    /// AI tasks verified on the first delivery (attempt count = 1 and VERIFIED).
    pub ai_first_pass: i64,
}

/// Per-day verified-task split (past year; data source for the GitHub-style
/// contribution grid). Historical tasks with a null verified_at are excluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollabDay {
    pub date: String,
    pub ai: i64,
    pub human: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CollabStats {
    pub items: Vec<CollabRequirementRow>,
    pub daily: Vec<CollabDay>,
}

#[async_trait]
pub trait DeliveryRepository: Send + Sync {
    async fn create(
        &self,
        decomposition_id: &str,
        task_id: &str,
        executor: ExecutorKind,
        target_runtime: Option<&str>,
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

    /// Human/AI collaboration stats (cross-context SQL aggregation; the in-memory
    /// implementation returns empty, only pg is meaningful). With requirement_id,
    /// scope to that requirement (requirement-detail view).
    async fn collab_stats(
        &self,
        project_id: &str,
        requirement_id: Option<&str>,
    ) -> Result<CollabStats, RepoError> {
        let _ = (project_id, requirement_id);
        Ok(CollabStats::default())
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError>;
}
