use async_trait::async_trait;
use thiserror::Error;

use api_runner::{Assertion, RequestSpec};
use probe::{ProbeOutcome, ProbeRequest};

use crate::domain::{
    AgentTarget, CaseSpec, DispatchTarget, ExecutionRecord, NewRunnerAgent, RemoteResult,
    RunnerAgent,
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait RunnerAgentStore: Send + Sync {
    async fn insert(
        &self,
        a: &NewRunnerAgent,
        protocols: &[String],
    ) -> Result<RunnerAgent, PortError>;
    async fn list(&self) -> Result<Vec<RunnerAgent>, PortError>;
    async fn dispatch_target(&self, id: &str) -> Result<Option<DispatchTarget>, PortError>;
    async fn agents_for_protocol(
        &self,
        protocol: &str,
    ) -> Result<Vec<AgentTarget>, PortError>;
    async fn set_protocols(&self, id: &str, protocols: &[String]) -> Result<bool, PortError>;
}

#[async_trait]
pub trait AgentCapabilities: Send + Sync {
    async fn protocols(&self, target: &DispatchTarget) -> Result<Vec<String>, PortError>;
}

#[async_trait]
pub trait RemoteRunner: Send + Sync {
    async fn run(
        &self,
        target: &DispatchTarget,
        request: &RequestSpec,
        assertions: &[Assertion],
    ) -> Result<RemoteResult, PortError>;
}

#[async_trait]
pub trait RemoteProbe: Send + Sync {
    async fn probe(
        &self,
        target: &DispatchTarget,
        req: &ProbeRequest,
    ) -> Result<ProbeOutcome, PortError>;
}

#[async_trait]
pub trait CaseSpecSource: Send + Sync {
    async fn spec_of(&self, case_id: &str) -> Result<Option<CaseSpec>, PortError>;
}

#[async_trait]
pub trait ExecutionStore: Send + Sync {
    async fn record(
        &self,
        agent_id: &str,
        method: &str,
        url: &str,
        result: &RemoteResult,
    ) -> Result<(), PortError>;

    async fn list_by_agent(
        &self,
        agent_id: &str,
        limit: u32,
    ) -> Result<Vec<ExecutionRecord>, PortError>;
}
