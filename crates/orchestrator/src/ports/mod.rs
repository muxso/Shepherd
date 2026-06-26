use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrchError {
    #[error("gateway error: {0}")]
    Gateway(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskTarget {
    Running,
    Delivered,
    Verified,
    Failed,
}

#[async_trait]
pub trait TaskGateway: Send + Sync {
    async fn requirement_of(
        &self,
        decomposition_id: &str,
    ) -> Result<Option<(String, u32)>, OrchError>;

    async fn advance_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
        target: TaskTarget,
    ) -> Result<(), OrchError>;

    async fn task_criteria(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<String>, OrchError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliverableView {
    pub kind: String,
    pub reference: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub passed: bool,
    pub reason: String,
}

#[async_trait]
pub trait Judge: Send + Sync {
    async fn judge(&self, criteria: &[String], deliverable: &DeliverableView) -> Verdict;
}

#[async_trait]
pub trait Reviser: Send + Sync {
    async fn revise(
        &self,
        decomposition_id: &str,
        task_id: &str,
        criteria: &[String],
        previous: &DeliverableView,
        feedback: &str,
    ) -> Result<DeliverableView, OrchError>;
}

#[async_trait]
pub trait VerificationGateway: Send + Sync {
    async fn find_verification(
        &self,
        requirement_id: &str,
        version: u32,
    ) -> Result<Option<String>, OrchError>;

    /// 须在 `sync` 前调用 —— 否则覆盖链为空,sync 无链可更,验证永远停在 UNCOVERED。
    async fn link(
        &self,
        verification_id: &str,
        decomposition_id: &str,
        task_id: &str,
        criteria_texts: &[String],
    ) -> Result<(), OrchError>;

    async fn sync(
        &self,
        verification_id: &str,
        decomposition_id: &str,
        task_id: &str,
        satisfied: bool,
    ) -> Result<(), OrchError>;
}
