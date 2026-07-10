use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{Deliverable, ExecutorKind};
use crate::ports::EventSink;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExecError {
    #[error("executor backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkSpec {
    pub attempt_id: String,
    pub decomposition_id: String,
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
    pub executor: ExecutorKind,
    pub context: Option<String>,
    pub instructions: Option<String>,
    /// 定向派发:指定某个注册 runtime 的 name(跨重连稳定;id 每次注册会变),
    /// None = 该能力的任意 runtime 都可认领。
    pub target_runtime: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    Accepted { run_id: String },
    Completed { deliverable: Deliverable },
}

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn dispatch(
        &self,
        spec: &WorkSpec,
        sink: &dyn EventSink,
    ) -> Result<DispatchOutcome, ExecError>;
}
