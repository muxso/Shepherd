//! reqwest 远程派发:把 `{request, assertions}` POST 给 agent 的 `/run`(带 token),回传结果。

use async_trait::async_trait;
use serde::Serialize;

use api_runner::{Assertion, RequestSpec};

use crate::domain::{DispatchTarget, RemoteResult};
use crate::ports::{PortError, RemoteRunner};

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
        // no_proxy:中央直连 agent(通常内网),不被全局代理劫持。
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
