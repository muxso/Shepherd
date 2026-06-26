use async_trait::async_trait;

use crate::domain::{Deliverable, DeliverableKind, EventKind, NewExecutionEvent};
use crate::ports::{AgentExecutor, DispatchOutcome, EventSink, ExecError, WorkSpec};

#[derive(Clone, Default)]
pub struct EchoAgentExecutor;

impl EchoAgentExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AgentExecutor for EchoAgentExecutor {
    async fn dispatch(
        &self,
        spec: &WorkSpec,
        sink: &dyn EventSink,
    ) -> Result<DispatchOutcome, ExecError> {
        if let Ok(ev) = NewExecutionEvent::new(
            EventKind::Log,
            &format!("echo executor: no real agent, completing {}", spec.title),
            None,
        ) {
            sink.emit(ev).await;
        }
        Ok(DispatchOutcome::Completed {
            deliverable: Deliverable {
                kind: DeliverableKind::Diff,
                reference: format!("stub://{}/{}", spec.decomposition_id, spec.task_id),
                summary: format!("[{}] echo: {}", spec.executor.as_str(), spec.title),
            },
        })
    }
}

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
    async fn dispatch(
        &self,
        _spec: &WorkSpec,
        _sink: &dyn EventSink,
    ) -> Result<DispatchOutcome, ExecError> {
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
