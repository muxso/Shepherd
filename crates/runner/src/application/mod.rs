//! 应用层:注册/列出 agent + 把用例派给指定 agent 执行。

use std::sync::Arc;

use thiserror::Error;

use api_runner::{Assertion, RequestSpec};

use crate::domain::{AgentError, NewRunnerAgent, RemoteResult, RunnerAgent};
use crate::ports::{PortError, RemoteRunner, RunnerAgentStore};

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
}

impl RunnerService {
    pub fn new(store: Arc<dyn RunnerAgentStore>, remote: Arc<dyn RemoteRunner>) -> Self {
        Self { store, remote }
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

    /// 把自包含用例派给某 agent 就地执行,回传结果。
    pub async fn run_via(
        &self,
        agent_id: &str,
        request: &RequestSpec,
        assertions: &[Assertion],
    ) -> Result<RemoteResult, RunViaAgentError> {
        let target =
            self.store.dispatch_target(agent_id).await?.ok_or(RunViaAgentError::AgentNotFound)?;
        Ok(self.remote.run(&target, request, assertions).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{InMemoryAgentStore, StubRemoteRunner};
    use api_runner::HttpMethod;

    fn svc() -> (RunnerService, Arc<InMemoryAgentStore>) {
        let store = Arc::new(InMemoryAgentStore::new());
        let remote = Arc::new(StubRemoteRunner::success());
        (RunnerService::new(store.clone(), remote), store)
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
