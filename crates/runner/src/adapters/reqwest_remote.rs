use async_trait::async_trait;
use serde::Serialize;

use api_runner::{Assertion, RequestSpec};
use probe::{ProbeOutcome, ProbeRequest};

use crate::domain::{DispatchTarget, RemoteResult};
use crate::ports::{AgentCapabilities, PortError, RemoteProbe, RemoteRunner};

#[derive(Serialize)]
struct RunPayload<'a> {
    request: &'a RequestSpec,
    assertions: &'a [Assertion],
}

#[derive(Clone)]
pub struct ReqwestRemoteRunner {
    client: reqwest::Client,
}

impl Default for ReqwestRemoteRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl ReqwestRemoteRunner {
    pub fn new() -> Self {
        // no_proxy:中央直连内网 agent,勿被全局代理劫持。
        Self { client: reqwest::Client::builder().no_proxy().build().unwrap_or_default() }
    }
}

#[async_trait]
impl RemoteRunner for ReqwestRemoteRunner {
    async fn run(
        &self,
        target: &DispatchTarget,
        request: &RequestSpec,
        assertions: &[Assertion],
    ) -> Result<RemoteResult, PortError> {
        let url = format!("{}/run", target.base_url.trim_end_matches('/'));
        let mut rb = self.client.post(&url).json(&RunPayload { request, assertions });
        if let Some(token) = &target.token {
            rb = rb.bearer_auth(token);
        }
        let resp = rb.send().await.map_err(|e| PortError::Backend(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(PortError::Backend(format!("agent HTTP {}", resp.status())));
        }
        resp.json::<RemoteResult>().await.map_err(|e| PortError::Backend(e.to_string()))
    }
}

#[derive(Clone)]
pub struct ReqwestRemoteProbe {
    client: reqwest::Client,
}

impl Default for ReqwestRemoteProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl ReqwestRemoteProbe {
    pub fn new() -> Self {
        Self { client: reqwest::Client::builder().no_proxy().build().unwrap_or_default() }
    }
}

#[async_trait]
impl RemoteProbe for ReqwestRemoteProbe {
    async fn probe(
        &self,
        target: &DispatchTarget,
        req: &ProbeRequest,
    ) -> Result<ProbeOutcome, PortError> {
        let url = format!("{}/probe", target.base_url.trim_end_matches('/'));
        let mut rb = self.client.post(&url).json(req);
        if let Some(token) = &target.token {
            rb = rb.bearer_auth(token);
        }
        let resp = rb.send().await.map_err(|e| PortError::Backend(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(PortError::Backend(format!("agent HTTP {}", resp.status())));
        }
        resp.json::<ProbeOutcome>().await.map_err(|e| PortError::Backend(e.to_string()))
    }
}

#[async_trait]
impl AgentCapabilities for ReqwestRemoteProbe {
    async fn protocols(&self, target: &DispatchTarget) -> Result<Vec<String>, PortError> {
        let url = format!("{}/protocols", target.base_url.trim_end_matches('/'));
        let mut rb = self.client.get(&url);
        if let Some(token) = &target.token {
            rb = rb.bearer_auth(token);
        }
        let resp = rb.send().await.map_err(|e| PortError::Backend(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(PortError::Backend(format!("agent HTTP {}", resp.status())));
        }
        resp.json::<Vec<String>>().await.map_err(|e| PortError::Backend(e.to_string()))
    }
}
