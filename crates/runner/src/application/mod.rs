//! 应用层:注册/列出 agent + 把用例派给指定 agent 执行。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use thiserror::Error;

use api_runner::{Assertion, RequestSpec};
use probe::{ProbeOutcome, ProbeRequest};

use crate::domain::{AgentError, ExecutionRecord, NewRunnerAgent, RemoteResult, RunnerAgent};
use crate::ports::{
    AgentCapabilities, CaseSpecSource, ExecutionStore, PortError, RemoteProbe, RemoteRunner,
    RunnerAgentStore,
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RegisterError {
    #[error(transparent)]
    Validation(#[from] AgentError),
    #[error(transparent)]
    Backend(#[from] PortError),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunViaAgentError {
    #[error("agent not found or disabled")]
    AgentNotFound,
    #[error(transparent)]
    Backend(#[from] PortError),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunCaseError {
    #[error("agent not found or disabled")]
    AgentNotFound,
    #[error("case not found")]
    CaseNotFound,
    #[error(transparent)]
    Backend(#[from] PortError),
}

impl From<RunViaAgentError> for RunCaseError {
    fn from(e: RunViaAgentError) -> Self {
        match e {
            RunViaAgentError::AgentNotFound => Self::AgentNotFound,
            RunViaAgentError::Backend(b) => Self::Backend(b),
        }
    }
}

/// 把协议无关的 ProbeOutcome 映射到执行历史用的 RemoteResult(复用同一张表)。
fn probe_to_remote(o: &ProbeOutcome) -> RemoteResult {
    RemoteResult {
        outcome: if o.success { "SUCCESS".to_string() } else { "ERROR".to_string() },
        status: o.status.and_then(|s| u16::try_from(s).ok()),
        elapsed_ms: Some(o.latency_ms),
        failures: o.failures.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunProbeError {
    #[error("no enabled agent supports protocol `{0}`")]
    NoAgent(String),
    #[error(transparent)]
    Backend(#[from] PortError),
}

/// 「按协议选 agent 派发」的结果:选中的 agent + 探测判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeReport {
    pub agent_id: String,
    pub agent_name: String,
    pub outcome: ProbeOutcome,
}

#[derive(Clone)]
pub struct RunnerService {
    store: Arc<dyn RunnerAgentStore>,
    remote: Arc<dyn RemoteRunner>,
    probe: Arc<dyn RemoteProbe>,
    capabilities: Arc<dyn AgentCapabilities>,
    executions: Arc<dyn ExecutionStore>,
    cases: Arc<dyn CaseSpecSource>,
    /// 多候选时的轮询游标(简单负载分摊)。
    rr: Arc<AtomicUsize>,
}

impl RunnerService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: Arc<dyn RunnerAgentStore>,
        remote: Arc<dyn RemoteRunner>,
        probe: Arc<dyn RemoteProbe>,
        capabilities: Arc<dyn AgentCapabilities>,
        executions: Arc<dyn ExecutionStore>,
        cases: Arc<dyn CaseSpecSource>,
    ) -> Self {
        Self {
            store,
            remote,
            probe,
            capabilities,
            executions,
            cases,
            rr: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// 注册 agent:先拉一次它的 `/protocols` 作能力快照(拉不到则空,不阻断注册),再落库。
    pub async fn register(
        &self,
        name: &str,
        base_url: &str,
        token: Option<String>,
        enabled: bool,
    ) -> Result<RunnerAgent, RegisterError> {
        let new = NewRunnerAgent::new(name, base_url, token, enabled)?;
        let target = crate::domain::DispatchTarget {
            base_url: new.base_url.clone(),
            token: new.token.clone(),
        };
        let protocols = self.capabilities.protocols(&target).await.unwrap_or_default();
        Ok(self.store.insert(&new, &protocols).await?)
    }

    /// 重新探测某 agent 的协议能力并写回(agent 换了 feature/重建后用)。
    pub async fn refresh_capabilities(
        &self,
        agent_id: &str,
    ) -> Result<Vec<String>, RunViaAgentError> {
        let target =
            self.store.dispatch_target(agent_id).await?.ok_or(RunViaAgentError::AgentNotFound)?;
        let protocols = self.capabilities.protocols(&target).await?;
        self.store.set_protocols(agent_id, &protocols).await?;
        Ok(protocols)
    }

    /// 按协议选一个支持该协议的 agent(多候选轮询),把 ProbeRequest 派给它的 `/probe` 执行。
    /// 这就是「中央按协议选 agent」:用户只说协议,中央据各 agent 能力路由到合适环境。
    pub async fn run_probe(&self, req: &ProbeRequest) -> Result<ProbeReport, RunProbeError> {
        let candidates = self.store.agents_for_protocol(&req.protocol).await?;
        if candidates.is_empty() {
            return Err(RunProbeError::NoAgent(req.protocol.clone()));
        }
        let idx = self.rr.fetch_add(1, Ordering::Relaxed) % candidates.len();
        let chosen = &candidates[idx];
        let outcome = self.probe.probe(&chosen.target, req).await?;
        // 复用执行历史表:protocol 入 method 列,target 入 url 列。
        let _ = self
            .executions
            .record(&chosen.id, &req.protocol, &req.target, &probe_to_remote(&outcome))
            .await;
        Ok(ProbeReport {
            agent_id: chosen.id.clone(),
            agent_name: chosen.name.clone(),
            outcome,
        })
    }

    pub async fn list(&self) -> Result<Vec<RunnerAgent>, PortError> {
        self.store.list().await
    }

    /// 把自包含用例派给某 agent 就地执行,回传结果。结果同时存档(尽力而为,不影响返回)。
    pub async fn run_via(
        &self,
        agent_id: &str,
        request: &RequestSpec,
        assertions: &[Assertion],
    ) -> Result<RemoteResult, RunViaAgentError> {
        let target =
            self.store.dispatch_target(agent_id).await?.ok_or(RunViaAgentError::AgentNotFound)?;
        let result = self.remote.run(&target, request, assertions).await?;
        let _ = self
            .executions
            .record(agent_id, request.method.as_str(), &request.url, &result)
            .await;
        Ok(result)
    }

    /// 把**已存储的用例**(case_id)解析为请求+断言,派给某 agent 就地执行(并存档)。
    pub async fn run_case(
        &self,
        agent_id: &str,
        case_id: &str,
    ) -> Result<RemoteResult, RunCaseError> {
        let spec = self.cases.spec_of(case_id).await?.ok_or(RunCaseError::CaseNotFound)?;
        Ok(self.run_via(agent_id, &spec.request, &spec.assertions).await?)
    }

    /// 某 agent 的最近执行历史。
    pub async fn executions(
        &self,
        agent_id: &str,
        limit: u32,
    ) -> Result<Vec<ExecutionRecord>, PortError> {
        self.executions.list_by_agent(agent_id, limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{
        InMemoryAgentStore, InMemoryCaseSpecSource, InMemoryExecutionStore, StubCapabilities,
        StubRemoteProbe, StubRemoteRunner,
    };
    use api_runner::HttpMethod;

    fn svc() -> (RunnerService, Arc<InMemoryAgentStore>, Arc<InMemoryCaseSpecSource>) {
        let (svc, store, cases, _caps) = svc_full();
        (svc, store, cases)
    }

    fn svc_full() -> (
        RunnerService,
        Arc<InMemoryAgentStore>,
        Arc<InMemoryCaseSpecSource>,
        Arc<StubCapabilities>,
    ) {
        let store = Arc::new(InMemoryAgentStore::new());
        let remote = Arc::new(StubRemoteRunner::success());
        let probe = Arc::new(StubRemoteProbe);
        let caps = Arc::new(StubCapabilities::new());
        let execs = Arc::new(InMemoryExecutionStore::new());
        let cases = Arc::new(InMemoryCaseSpecSource::new());
        let svc = RunnerService::new(
            store.clone(),
            remote,
            probe,
            caps.clone(),
            execs,
            cases.clone(),
        );
        (svc, store, cases, caps)
    }

    fn spec() -> RequestSpec {
        RequestSpec { method: HttpMethod::Get, url: "http://t/x".into(), headers: vec![], body: None }
    }

    #[tokio::test]
    async fn register_list_and_run_via() {
        let (svc, _store, _cases) = svc();
        let a = svc.register("测试环境", "http://10.0.0.5:9100", Some("t".into()), true).await.expect("reg");
        assert_eq!(svc.list().await.expect("list").len(), 1);

        let res = svc.run_via(&a.id, &spec(), &[Assertion::StatusIs(200)]).await.expect("run");
        assert_eq!(res.outcome, "SUCCESS");

        // 派发后应有一条执行历史。
        let execs = svc.executions(&a.id, 10).await.expect("execs");
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].outcome, "SUCCESS");
        assert_eq!(execs[0].method, "GET");
        assert_eq!(execs[0].url, "http://t/x");
    }

    #[tokio::test]
    async fn run_via_unknown_agent_is_not_found() {
        let (svc, _s, _c) = svc();
        assert_eq!(
            svc.run_via("ghost", &spec(), &[]).await.unwrap_err(),
            RunViaAgentError::AgentNotFound
        );
    }

    #[tokio::test]
    async fn register_rejects_blank() {
        let (svc, _s, _c) = svc();
        assert!(matches!(
            svc.register(" ", "http://x", None, true).await.unwrap_err(),
            RegisterError::Validation(AgentError::EmptyName)
        ));
    }

    #[tokio::test]
    async fn run_stored_case_via_agent() {
        let (svc, _s, cases) = svc();
        let a = svc.register("环境A", "http://a:9100", None, true).await.expect("reg");
        cases.seed("case1", spec(), vec![Assertion::StatusIs(200)]);

        let res = svc.run_case(&a.id, "case1").await.expect("run case");
        assert_eq!(res.outcome, "SUCCESS");
        // 派发后入档
        assert_eq!(svc.executions(&a.id, 10).await.expect("e").len(), 1);
    }

    #[tokio::test]
    async fn run_unknown_case_is_not_found() {
        let (svc, _s, _c) = svc();
        let a = svc.register("环境A", "http://a:9100", None, true).await.expect("reg");
        assert_eq!(svc.run_case(&a.id, "ghost").await.unwrap_err(), RunCaseError::CaseNotFound);
    }

    #[tokio::test]
    async fn register_snapshots_protocols_and_routes_by_protocol() {
        let (svc, _s, _c, caps) = svc_full();
        // 注册时拉到的能力被快照入库。
        caps.set("http://grpc-env:9100", &["http", "grpc"]);
        caps.set("http://sql-env:9100", &["http", "sql"]);
        let g = svc.register("gRPC环境", "http://grpc-env:9100", None, true).await.expect("reg");
        assert_eq!(g.protocols, vec!["http".to_string(), "grpc".to_string()]);
        svc.register("SQL环境", "http://sql-env:9100", None, true).await.expect("reg");

        // 只给协议 → 选支持该协议的 agent。
        let rep = svc.run_probe(&probe_req("grpc")).await.expect("probe");
        assert_eq!(rep.agent_name, "gRPC环境");
        assert!(rep.outcome.success);

        let rep = svc.run_probe(&probe_req("sql")).await.expect("probe");
        assert_eq!(rep.agent_name, "SQL环境");

        // 派发后入执行历史(protocol 记在 method 列)。
        assert_eq!(svc.executions(&rep.agent_id, 10).await.expect("e")[0].method, "sql");

        // 无 agent 支持 redis。
        assert_eq!(
            svc.run_probe(&probe_req("redis")).await.unwrap_err(),
            RunProbeError::NoAgent("redis".to_string())
        );
    }

    #[tokio::test]
    async fn round_robin_spreads_across_candidates() {
        let (svc, _s, _c, caps) = svc_full();
        caps.set("http://a:9100", &["grpc"]);
        caps.set("http://b:9100", &["grpc"]);
        svc.register("A", "http://a:9100", None, true).await.expect("reg");
        svc.register("B", "http://b:9100", None, true).await.expect("reg");
        // 两个都支持 grpc;连续派发应轮询命中两个不同 agent。
        let first = svc.run_probe(&probe_req("grpc")).await.expect("p").agent_name;
        let second = svc.run_probe(&probe_req("grpc")).await.expect("p").agent_name;
        assert_ne!(first, second);
    }

    fn probe_req(protocol: &str) -> ProbeRequest {
        ProbeRequest {
            protocol: protocol.into(),
            target: "t".into(),
            payload: None,
            metadata: Default::default(),
            assertions: vec![],
        }
    }
}
