//! 内存 agent 注册表 + 桩远程派发(测试用)。

use std::sync::Mutex;

use async_trait::async_trait;

use api_runner::{Assertion, RequestSpec};

use crate::domain::{
    CaseSpec, DispatchTarget, ExecutionRecord, NewRunnerAgent, RemoteResult, RunnerAgent,
};
use crate::ports::{CaseSpecSource, ExecutionStore, PortError, RemoteRunner, RunnerAgentStore};

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

/// 内存执行历史(测试)。
#[derive(Default)]
pub struct InMemoryExecutionStore {
    records: Mutex<Vec<ExecutionRecord>>,
}

impl InMemoryExecutionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ExecutionStore for InMemoryExecutionStore {
    async fn record(
        &self,
        agent_id: &str,
        method: &str,
        url: &str,
        result: &RemoteResult,
    ) -> Result<(), PortError> {
        let mut g = self.records.lock().map_err(|e| PortError::Backend(e.to_string()))?;
        let id = format!("e{}", g.len() + 1);
        g.push(ExecutionRecord {
            id,
            agent_id: agent_id.to_string(),
            method: method.to_string(),
            url: url.to_string(),
            outcome: result.outcome.clone(),
            status: result.status,
            elapsed_ms: result.elapsed_ms,
            failures: result.failures.clone(),
            executed_at: "1970-01-01T00:00:00Z".to_string(),
        });
        Ok(())
    }

    async fn list_by_agent(
        &self,
        agent_id: &str,
        limit: u32,
    ) -> Result<Vec<ExecutionRecord>, PortError> {
        let g = self.records.lock().map_err(|e| PortError::Backend(e.to_string()))?;
        Ok(g.iter()
            .filter(|r| r.agent_id == agent_id)
            .rev()
            .take(limit as usize)
            .cloned()
            .collect())
    }
}

/// 内存用例规格来源(测试)。`seed` 预置 case_id → 规格。
#[derive(Default)]
pub struct InMemoryCaseSpecSource {
    specs: Mutex<Vec<(String, CaseSpec)>>,
}

impl InMemoryCaseSpecSource {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn seed(&self, case_id: &str, request: RequestSpec, assertions: Vec<Assertion>) {
        self.specs.lock().expect("lock").push((case_id.to_string(), CaseSpec { request, assertions }));
    }
}

#[async_trait]
impl CaseSpecSource for InMemoryCaseSpecSource {
    async fn spec_of(&self, case_id: &str) -> Result<Option<CaseSpec>, PortError> {
        Ok(self
            .specs
            .lock()
            .map_err(|e| PortError::Backend(e.to_string()))?
            .iter()
            .find(|(id, _)| id == case_id)
            .map(|(_, s)| s.clone()))
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
