//! 端口层:agent 注册表 + 远程派发。

use async_trait::async_trait;
use thiserror::Error;

use api_runner::{Assertion, RequestSpec};

use crate::domain::{DispatchTarget, NewRunnerAgent, RemoteResult, RunnerAgent};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("backend error: {0}")]
    Backend(String),
}

/// agent 注册表(增/列/取派发目标)。
#[async_trait]
pub trait RunnerAgentStore: Send + Sync {
    async fn insert(&self, a: &NewRunnerAgent) -> Result<RunnerAgent, PortError>;
    async fn list(&self) -> Result<Vec<RunnerAgent>, PortError>;
    /// 取派发目标(含 token);不存在/禁用返回 None。
    async fn dispatch_target(&self, id: &str) -> Result<Option<DispatchTarget>, PortError>;
}

/// 远程派发:把自包含用例发给某 agent 就地执行,回传结果。
#[async_trait]
pub trait RemoteRunner: Send + Sync {
    async fn run(
        &self,
        target: &DispatchTarget,
        request: &RequestSpec,
        assertions: &[Assertion],
    ) -> Result<RemoteResult, PortError>;
}
