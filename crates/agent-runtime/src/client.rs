//! 与 server 的出站 HTTP 客户端:login / register / heartbeat / claim(长轮询)/ 回调。
//! 全部出站(agent 无公网入站)。

use serde_json::json;

use crate::events::{ExecEvent, ProgressSink};
use crate::models::WorkSpec;

pub struct ServerClient {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl ServerClient {
    /// 用 admin 凭证登录拿 token(同 fleet-runtime.sh)。
    pub async fn login(base: &str, user: &str, pass: &str) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder().build()?;
        let resp: serde_json::Value = http
            .post(format!("{base}/auth/login"))
            .json(&json!({"username": user, "password": pass}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let token = resp
            .get("token")
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow::anyhow!("login: no token"))?
            .to_string();
        Ok(Self { http, base: base.to_string(), token })
    }

    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        rb.bearer_auth(&self.token)
    }

    /// 注册 runtime,返回 runtimeId。
    pub async fn register(
        &self,
        name: &str,
        caps: &[String],
        max_concurrency: u32,
    ) -> anyhow::Result<String> {
        let resp: serde_json::Value = self
            .auth(self.http.post(format!("{}/agent/runtime", self.base)))
            .json(&json!({"name": name, "caps": caps, "maxConcurrency": max_concurrency}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.get("runtimeId").and_then(|v| v.as_str()).unwrap_or_default().to_string())
    }

    /// 心跳续约;404 → false(需重新注册)。
    pub async fn heartbeat(&self, runtime_id: &str) -> anyhow::Result<bool> {
        let code = self
            .auth(self.http.post(format!("{}/agent/runtime/{runtime_id}/heartbeat", self.base)))
            .send()
            .await?
            .status();
        Ok(code != reqwest::StatusCode::NOT_FOUND)
    }

    /// 长轮询认领一个任务;204 → None。
    pub async fn claim(&self, caps: &[String], runtime_id: &str) -> anyhow::Result<Option<WorkSpec>> {
        let caps_csv = caps.join(",");
        let resp = self
            .auth(self.http.get(format!(
                "{}/agent/work/claim?caps={caps_csv}&runtime={runtime_id}",
                self.base
            )))
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }
        let resp = resp.error_for_status()?;
        Ok(Some(resp.json().await?))
    }

    pub async fn post_event(&self, attempt_id: &str, ev: &ExecEvent) {
        let _ = self
            .auth(self.http.post(format!("{}/delivery/{attempt_id}/events", self.base)))
            .json(&json!({"kind": ev.kind, "message": ev.message}))
            .send()
            .await;
    }

    pub async fn complete(&self, attempt_id: &str, kind: &str, reference: &str, summary: &str) {
        let _ = self
            .auth(self.http.post(format!("{}/delivery/{attempt_id}/complete", self.base)))
            .json(&json!({"kind": kind, "reference": reference, "summary": summary}))
            .send()
            .await;
    }

    pub async fn fail(&self, attempt_id: &str, error: &str) {
        let _ = self
            .auth(self.http.post(format!("{}/delivery/{attempt_id}/fail", self.base)))
            .json(&json!({"error": error}))
            .send()
            .await;
    }

    /// design 模式:把设计稿回填到提案 → 进入待审。
    pub async fn post_design(&self, proposal_id: &str, doc: &str) -> anyhow::Result<()> {
        self.auth(self.http.post(format!("{}/proposal/{proposal_id}/design", self.base)))
            .json(&json!({"doc": doc}))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

/// 把进度事件实时 POST 到 `/delivery/{id}/events`(实现模式用)。
pub struct HttpSink<'a> {
    pub client: &'a ServerClient,
    pub attempt_id: String,
}

#[async_trait::async_trait]
impl ProgressSink for HttpSink<'_> {
    async fn emit(&self, ev: ExecEvent) {
        self.client.post_event(&self.attempt_id, &ev).await;
    }
}
