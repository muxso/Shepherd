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

/// 人机协同人效:某需求下已验收任务的 AI/人工 拆分。
/// 口径:任务 VERIFIED 且存在 DELIVERED 交付记录 = AI 交付;VERIFIED 无记录 = 人工交付。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollabRequirementRow {
    pub requirement_id: String,
    pub title: String,
    pub ai_tasks: i64,
    pub human_tasks: i64,
    pub ai_points: i64,
    pub human_points: i64,
}

/// 按周(周一日期)的验收通过任务数拆分;verified_at 为空的历史任务不计入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollabWeek {
    pub week: String,
    pub ai: i64,
    pub human: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CollabStats {
    pub items: Vec<CollabRequirementRow>,
    pub weekly: Vec<CollabWeek>,
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

    /// 人机协同人效统计(跨上下文 SQL 聚合;内存实现返回空,仅 pg 有意义)。
    async fn collab_stats(&self, project_id: &str) -> Result<CollabStats, RepoError> {
        let _ = project_id;
        Ok(CollabStats::default())
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError>;
}
