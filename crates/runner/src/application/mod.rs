//! 应用层:注册/列出 agent + 把用例派给指定 agent 执行。

use std::sync::Arc;

use thiserror::Error;

use api_runner::{Assertion, RequestSpec};

use crate::domain::{AgentError, ExecutionRecord, NewRunnerAgent, RemoteResult, RunnerAgent};
use crate::ports::{ExecutionStore, PortError, RemoteRunner, RunnerAgentStore};

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

#[derive(Clone)]
pub struct RunnerService {
    store: Arc<dyn RunnerAgentStore>,
    remote: Arc<dyn RemoteRunner>,
    executions: Arc<dyn ExecutionStore>,
}

impl RunnerService {
    pub fn new(
        store: Arc<dyn RunnerAgentStore>,
        remote: Arc<dyn RemoteRunner>,
        executions: Arc<dyn ExecutionStore>,
    ) -> Self {
        Self { store, remote, executions }
    }

    pub async fn register(
        &self,
        name: &str,
        base_url: &str,
        token: Option<String>,
        enabled: bool,
    ) -> Result<RunnerAgent, RegisterError> {
        let new = NewRunnerAgent::new(name, base_url, token, enabled)?;
        Ok(self.store.insert(&new).await?)
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
    use crate::adapters::{InMemoryAgentStore, InMemoryExecutionStore, StubRemoteRunner};
    use api_runner::HttpMethod;

    fn svc() -> (RunnerService, Arc<InMemoryAgentStore>) {
        let store = Arc::new(InMemoryAgentStore::new());
        let remote = Arc::new(StubRemoteRunner::success());
        let execs = Arc::new(InMemoryExecutionStore::new());
        (RunnerService::new(store.clone(), remote, execs), store)
    }

    fn spec() -> RequestSpec {
        RequestSpec { method: HttpMethod::Get, url: "http://t/x".into(), headers: vec![], body: None }
    }

    #[tokio::test]
    async fn register_list_and_run_via() {
        let (svc, _store) = svc();
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
        let (svc, _s) = svc();
        assert_eq!(
            svc.run_via("ghost", &spec(), &[]).await.unwrap_err(),
            RunViaAgentError::AgentNotFound
        );
    }

    #[tokio::test]
    async fn register_rejects_blank() {
        let (svc, _s) = svc();
        assert!(matches!(
            svc.register(" ", "http://x", None, true).await.unwrap_err(),
            RegisterError::Validation(AgentError::EmptyName)
        ));
    }
}
