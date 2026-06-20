//! 端口层:agent 注册表 + 远程派发。

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

/// agent 注册表(增/列/取派发目标/按协议筛选)。
#[async_trait]
pub trait RunnerAgentStore: Send + Sync {
    /// 注册 agent;`protocols` 为注册时从其 `/protocols` 拉到的能力快照。
    async fn insert(
        &self,
        a: &NewRunnerAgent,
        protocols: &[String],
    ) -> Result<RunnerAgent, PortError>;
    async fn list(&self) -> Result<Vec<RunnerAgent>, PortError>;
    /// 取派发目标(含 token);不存在/禁用返回 None。
    async fn dispatch_target(&self, id: &str) -> Result<Option<DispatchTarget>, PortError>;
    /// 取所有 enabled 且支持 `protocol` 的 agent 候选(含 token,供中央按协议选 agent)。
    async fn agents_for_protocol(
        &self,
        protocol: &str,
    ) -> Result<Vec<AgentTarget>, PortError>;
    /// 刷新某 agent 的协议能力快照(重拉 /protocols 后写回)。返回是否命中。
    async fn set_protocols(&self, id: &str, protocols: &[String]) -> Result<bool, PortError>;
}

/// 探测某 agent 自报的协议能力(GET /protocols)。注册/刷新时用。
#[async_trait]
pub trait AgentCapabilities: Send + Sync {
    async fn protocols(&self, target: &DispatchTarget) -> Result<Vec<String>, PortError>;
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

/// 远程多协议探测:把 `ProbeRequest` 发给某 agent 的 `/probe` 就地执行,回传判定结果。
#[async_trait]
pub trait RemoteProbe: Send + Sync {
    async fn probe(
        &self,
        target: &DispatchTarget,
        req: &ProbeRequest,
    ) -> Result<ProbeOutcome, PortError>;
}

/// 已存储用例规格来源:据 case_id 解析出可执行的「请求 + 断言」。
#[async_trait]
pub trait CaseSpecSource: Send + Sync {
    async fn spec_of(&self, case_id: &str) -> Result<Option<CaseSpec>, PortError>;
}

/// 远程执行历史存档:记录每次派发结果 + 按 agent 查询。
#[async_trait]
pub trait ExecutionStore: Send + Sync {
    /// 记录一次派发执行(method/url 为被测目标,result 为 agent 回传)。
    async fn record(
        &self,
        agent_id: &str,
        method: &str,
        url: &str,
        result: &RemoteResult,
    ) -> Result<(), PortError>;

    /// 按 agent 取最近执行记录(时间倒序,最多 limit 条)。
    async fn list_by_agent(
        &self,
        agent_id: &str,
        limit: u32,
    ) -> Result<Vec<ExecutionRecord>, PortError>;
}
