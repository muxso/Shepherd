//! 桩执行者:
//! - `EchoAgentExecutor`:无真实 agent 时的默认实现,同步"完成"并回显任务(供本地/演示)。
//! - `StubAgentExecutor`:按预设行为返回 Accepted/Completed/错误,供测试驱动各分支。

use async_trait::async_trait;

use crate::domain::{Deliverable, DeliverableKind};
use crate::ports::{AgentExecutor, DispatchOutcome, ExecError, WorkSpec};

/// 默认桩:同步完成,交付物回显任务标题(reference 指向一个占位)。
#[derive(Clone, Default)]
pub struct EchoAgentExecutor;

impl EchoAgentExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AgentExecutor for EchoAgentExecutor {
    async fn dispatch(&self, spec: &WorkSpec) -> Result<DispatchOutcome, ExecError> {
        Ok(DispatchOutcome::Completed {
            deliverable: Deliverable {
                kind: DeliverableKind::Diff,
                reference: format!("stub://{}/{}", spec.decomposition_id, spec.task_id),
                summary: format!("[{}] echo: {}", spec.executor.as_str(), spec.title),
            },
        })
    }
}

/// 测试用可配置桩。
#[derive(Clone)]
pub enum StubBehavior {
    Accept { run_id: String },
    Complete { deliverable: Deliverable },
    Error { message: String },
}

#[derive(Clone)]
pub struct StubAgentExecutor {
    behavior: StubBehavior,
}

impl StubAgentExecutor {
    pub fn new(behavior: StubBehavior) -> Self {
        Self { behavior }
    }
}

#[async_trait]
impl AgentExecutor for StubAgentExecutor {
    async fn dispatch(&self, _spec: &WorkSpec) -> Result<DispatchOutcome, ExecError> {
        match &self.behavior {
            StubBehavior::Accept { run_id } => {
                Ok(DispatchOutcome::Accepted { run_id: run_id.clone() })
            }
            StubBehavior::Complete { deliverable } => {
                Ok(DispatchOutcome::Completed { deliverable: deliverable.clone() })
            }
            StubBehavior::Error { message } => Err(ExecError::Backend(message.clone())),
        }
    }
}
