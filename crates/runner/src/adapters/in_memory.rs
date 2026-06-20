//! 内存 agent 注册表 + 桩远程派发(测试用)。

use std::sync::Mutex;

use async_trait::async_trait;

use api_runner::{Assertion, RequestSpec};
use probe::{ProbeOutcome, ProbeRequest, RawProbe};

use crate::domain::{
    AgentTarget, CaseSpec, DispatchTarget, ExecutionRecord, NewRunnerAgent, RemoteResult,
    RunnerAgent,
};
use crate::ports::{
    AgentCapabilities, CaseSpecSource, ExecutionStore, PortError, RemoteProbe, RemoteRunner,
    RunnerAgentStore,
};

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
    async fn insert(
        &self,
        a: &NewRunnerAgent,
        protocols: &[String],
    ) -> Result<RunnerAgent, PortError> {
        let mut g = self.agents.lock().map_err(|e| PortError::Backend(e.to_string()))?;
        let view = RunnerAgent {
            id: format!("a{}", g.len() + 1),
            name: a.name.clone(),
            base_url: a.base_url.clone(),
            enabled: a.enabled,
            protocols: protocols.to_vec(),
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

    async fn agents_for_protocol(
        &self,
        protocol: &str,
    ) -> Result<Vec<AgentTarget>, PortError> {
        Ok(self
            .agents
            .lock()
            .map_err(|e| PortError::Backend(e.to_string()))?
            .iter()
            .filter(|(v, _)| v.enabled && v.protocols.iter().any(|p| p == protocol))
            .map(|(v, tok)| AgentTarget {
                id: v.id.clone(),
                name: v.name.clone(),
                target: DispatchTarget { base_url: v.base_url.clone(), token: tok.clone() },
            })
            .collect())
    }

    async fn set_protocols(&self, id: &str, protocols: &[String]) -> Result<bool, PortError> {
        let mut g = self.agents.lock().map_err(|e| PortError::Backend(e.to_string()))?;
        match g.iter_mut().find(|(v, _)| v.id == id) {
            Some((v, _)) => {
                v.protocols = protocols.to_vec();
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

/// 桩协议能力探测:按 base_url 预置该 agent 自报的协议(测试路由用)。
#[derive(Default)]
pub struct StubCapabilities {
    by_url: Mutex<Vec<(String, Vec<String>)>>,
}

impl StubCapabilities {
    pub fn new() -> Self {
        Self::default()
    }
    /// 预置:某 base_url 的 agent 自报支持这些协议。
    pub fn set(&self, base_url: &str, protocols: &[&str]) {
        self.by_url
            .lock()
            .expect("lock")
            .push((base_url.to_string(), protocols.iter().map(|s| s.to_string()).collect()));
    }
}

#[async_trait]
impl AgentCapabilities for StubCapabilities {
    async fn protocols(&self, target: &DispatchTarget) -> Result<Vec<String>, PortError> {
        Ok(self
            .by_url
            .lock()
            .map_err(|e| PortError::Backend(e.to_string()))?
            .iter()
            .find(|(u, _)| *u == target.base_url)
            .map(|(_, p)| p.clone())
            .unwrap_or_default())
    }
}

/// 桩远程探测:不发网络,据本地空注册表合成结果(回报传输成功 + 断言判定)。
pub struct StubRemoteProbe;

#[async_trait]
impl RemoteProbe for StubRemoteProbe {
    async fn probe(
        &self,
        _target: &DispatchTarget,
        req: &ProbeRequest,
    ) -> Result<ProbeOutcome, PortError> {
        // 模拟 agent 就地执行成功:transport_ok + 回显 protocol 作输出。
        let raw = RawProbe {
            transport_ok: true,
            status: Some(200),
            latency_ms: 1,
            output: Some(req.protocol.clone()),
            error: None,
        };
        Ok(ProbeOutcome::from_raw(raw, &req.assertions))
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
