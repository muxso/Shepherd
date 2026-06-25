//! 与 server 的出站 HTTP 客户端:login / register / heartbeat / claim(长轮询)/ 回调。
//! 全部出站(agent 无公网入站)。
//!
//! 健壮性约定(对齐成熟 pull 型 runner —— GitHub Actions runner / Buildkite agent):
//! - **每请求超时**:控制面 15s、认领长轮询 30s(server 端持有 ~20s),连接 10s;
//!   黑洞网络不再无限期 wedge 住 runtime。
//! - **终态上报必重试**:complete / fail / 设计稿回填丢一次就等于丢交付,故用有上限的
//!   指数退避重试;耗尽才放弃(server 侧到点 reclaim 兜底)。

use std::time::Duration;

use serde_json::json;

use crate::events::{ExecEvent, ProgressSink};
use crate::models::WorkSpec;

/// 控制面单请求超时(register / heartbeat / 回调)。
const CONTROL_TIMEOUT: Duration = Duration::from_secs(15);
/// 认领长轮询超时:须 > server 端持有时长(20s)。
const CLAIM_TIMEOUT: Duration = Duration::from_secs(30);
/// 终态上报重试次数(≈ 200ms→…→10s,总计约 30s)。
const REPORT_ATTEMPTS: u32 = 6;

pub struct ServerClient {
    http: reqwest::Client,
    base: String,
    token: String,
}

/// 有上限的指数退避重试(200ms 起,×2,封顶 10s)。`f` 须幂等。
async fn retry<T, F, Fut>(label: &str, attempts: u32, mut f: F) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let mut delay = Duration::from_millis(200);
    for i in 1..=attempts {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if i == attempts => return Err(e),
            Err(e) => {
                tracing::warn!("{label} 第 {i}/{attempts} 次失败,{delay:?} 后重试: {e}");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(10));
            }
        }
    }
    unreachable!("attempts >= 1")
}

impl ServerClient {
    /// 用 admin 凭证登录拿 token(同 fleet-runtime.sh)。
    pub async fn login(base: &str, user: &str, pass: &str) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build()?;
        let resp: serde_json::Value = http
            .post(format!("{base}/auth/login"))
            .timeout(CONTROL_TIMEOUT)
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
        rb.bearer_auth(&self.token).timeout(CONTROL_TIMEOUT)
    }

    /// 注册 runtime,返回 runtimeId。瞬时网络抖动自动重试。
    pub async fn register(
        &self,
        name: &str,
        caps: &[String],
        max_concurrency: u32,
    ) -> anyhow::Result<String> {
        retry("register", REPORT_ATTEMPTS, || async {
            let resp: serde_json::Value = self
                .auth(self.http.post(format!("{}/agent/runtime", self.base)))
                .json(&json!({"name": name, "caps": caps, "maxConcurrency": max_concurrency}))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            Ok(resp.get("runtimeId").and_then(|v| v.as_str()).unwrap_or_default().to_string())
        })
        .await
    }

    /// 心跳续约;404 → false(需重新注册)。单发(失败由心跳循环下一拍重试)。
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
            .timeout(CLAIM_TIMEOUT) // 覆盖控制面默认:长轮询须比 server 持有时长长
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }
        let resp = resp.error_for_status()?;
        Ok(Some(resp.json().await?))
    }

    /// 进度事件:尽力而为单发(丢一条进度不致命,不阻塞流式)。
    pub async fn post_event(&self, attempt_id: &str, ev: &ExecEvent) {
        let _ = self
            .auth(self.http.post(format!("{}/delivery/{attempt_id}/events", self.base)))
            .json(&json!({"kind": ev.kind, "message": ev.message}))
            .send()
            .await;
    }

    /// 交付完成(终态):必重试,耗尽才放弃 → server 到点 reclaim 兜底。
    pub async fn complete(
        &self,
        attempt_id: &str,
        kind: &str,
        reference: &str,
        summary: &str,
    ) -> anyhow::Result<()> {
        retry("complete", REPORT_ATTEMPTS, || async {
            self.auth(self.http.post(format!("{}/delivery/{attempt_id}/complete", self.base)))
                .json(&json!({"kind": kind, "reference": reference, "summary": summary}))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        })
        .await
    }

    /// 交付失败(终态):同样必重试。
    pub async fn fail(&self, attempt_id: &str, error: &str) -> anyhow::Result<()> {
        retry("fail", REPORT_ATTEMPTS, || async {
            self.auth(self.http.post(format!("{}/delivery/{attempt_id}/fail", self.base)))
                .json(&json!({"error": error}))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        })
        .await
    }

    /// design 模式:把设计稿回填到提案 → 进入待审(终态,必重试)。
    pub async fn post_design(&self, proposal_id: &str, doc: &str) -> anyhow::Result<()> {
        retry("post_design", REPORT_ATTEMPTS, || async {
            self.auth(self.http.post(format!("{}/proposal/{proposal_id}/design", self.base)))
                .json(&json!({"doc": doc}))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        })
        .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test(start_paused = true)]
    async fn retry_succeeds_after_transient_failures() {
        let calls = AtomicU32::new(0);
        let out = retry("t", 5, || async {
            let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
            if n < 3 {
                Err(anyhow::anyhow!("transient {n}"))
            } else {
                Ok(n)
            }
        })
        .await
        .expect("eventually ok");
        assert_eq!(out, 3);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn retry_gives_up_after_exhausting_attempts() {
        let calls = AtomicU32::new(0);
        let err = retry("t", 3, || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(anyhow::anyhow!("always"))
        })
        .await
        .expect_err("should exhaust");
        assert!(err.to_string().contains("always"));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
}
