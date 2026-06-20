//! 内存 agent 注册表 + 桩远程派发(测试用)。

use std::sync::Mutex;

use async_trait::async_trait;

use api_runner::{Assertion, RequestSpec};

use crate::domain::{DispatchTarget, NewRunnerAgent, RemoteResult, RunnerAgent};
use crate::ports::{PortError, RemoteRunner, RunnerAgentStore};

#[derive(Default)]
pub struct InMemoryAgentStore {
    agents: Mutex<Vec<(RunnerAgent, Option<String>)>>,
}

impl InMemoryAgentStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl RunnerAgentStore for InMemoryAgentStore {
    async fn insert(&self, a: &NewRunnerAgent) -> Result<RunnerAgent, PortError> {
        let mut g = self.agents.lock().map_err(|e| PortError::Backend(e.to_string()))?;
        let view = RunnerAgent {
            id: format!("a{}", g.len() + 1),
            name: a.name.clone(),
            base_url: a.base_url.clone(),
            enabled: a.enabled,
        };
        g.push((view.clone(), a.token.clone()));
        Ok(view)
    }

    async fn list(&self) -> Result<Vec<RunnerAgent>, PortError> {
        Ok(self
            .agents
            .lock()
            .map_err(|e| PortError::Backend(e.to_string()))?
            .iter()
            .map(|(v, _)| v.clone())
            .collect())
    }

    async fn dispatch_target(&self, id: &str) -> Result<Option<DispatchTarget>, PortError> {
        Ok(self
            .agents
            .lock()
            .map_err(|e| PortError::Backend(e.to_string()))?
            .iter()
            .find(|(v, _)| v.id == id && v.enabled)
            .map(|(v, tok)| DispatchTarget { base_url: v.base_url.clone(), token: tok.clone() }))
    }
}

/// 桩远程派发:不发网络,直接回固定结果(测试 RunnerService 编排用)。
pub struct StubRemoteRunner {
    result: RemoteResult,
}

impl StubRemoteRunner {
    pub fn success() -> Self {
        Self {
            result: RemoteResult {
                outcome: "SUCCESS".into(),
                status: Some(200),
                elapsed_ms: Some(3),
                failures: vec![],
            },
        }
    }
}

#[async_trait]
impl RemoteRunner for StubRemoteRunner {
    async fn run(
        &self,
        _target: &DispatchTarget,
        _request: &RequestSpec,
        _assertions: &[Assertion],
    ) -> Result<RemoteResult, PortError> {
        Ok(self.result.clone())
    }
}
