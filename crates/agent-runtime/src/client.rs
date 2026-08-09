use std::time::Duration;

use serde_json::json;

use crate::events::{ExecEvent, ProgressSink};
use crate::models::WorkSpec;

const CONTROL_TIMEOUT: Duration = Duration::from_secs(15);
// Claim long-poll timeout must exceed the server-side hold (~20s).
const CLAIM_TIMEOUT: Duration = Duration::from_secs(30);
const REPORT_ATTEMPTS: u32 = 6;

pub struct ServerClient {
    http: reqwest::Client,
    base: String,
    /// Static API key (SHEPHERD_AGENT_KEY): the only credential — no login, no refresh; a 401 means revoked.
    key: String,
}

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
                tracing::warn!("{label} attempt {i}/{attempts} failed, retrying in {delay:?}: {e}");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(10));
            }
        }
    }
    unreachable!("attempts >= 1")
}

impl ServerClient {
    /// Uses the static API key (`sak_…`) directly as bearer: no login/refresh; revocation kills it.
    pub fn with_api_key(base: &str, key: &str) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build()?;
        Ok(Self { http, base: base.to_string(), key: key.to_string() })
    }

    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        rb.bearer_auth(&self.key).timeout(CONTROL_TIMEOUT)
    }

    /// A 401 means the key is revoked/invalid; retrying won't recover, so log explicit guidance.
    fn key_revoked(&self, status: reqwest::StatusCode) -> bool {
        let revoked = status == reqwest::StatusCode::UNAUTHORIZED;
        if revoked {
            tracing::error!(
                "API key revoked or invalid (HTTP 401): ask an admin to reissue it via POST /system/apikey and update SHEPHERD_AGENT_KEY"
            );
        }
        revoked
    }

    /// Stand-in for error_for_status: maps 401 to a readable "key revoked" error, keeps status errors otherwise.
    fn ensure_ok(&self, resp: reqwest::Response) -> anyhow::Result<reqwest::Response> {
        if self.key_revoked(resp.status()) {
            anyhow::bail!("API key revoked or invalid");
        }
        Ok(resp.error_for_status()?)
    }

    pub async fn register(
        &self,
        name: &str,
        caps: &[String],
        max_concurrency: u32,
    ) -> anyhow::Result<String> {
        retry("register", REPORT_ATTEMPTS, || async {
            let resp = self
                .auth(self.http.post(format!("{}/agent/runtime", self.base)))
                .json(&json!({"name": name, "caps": caps, "maxConcurrency": max_concurrency}))
                .send()
                .await?;
            let resp: serde_json::Value = self.ensure_ok(resp)?.json().await?;
            Ok(resp.get("runtimeId").and_then(|v| v.as_str()).unwrap_or_default().to_string())
        })
        .await
    }

    pub async fn heartbeat(&self, runtime_id: &str) -> anyhow::Result<bool> {
        let code = self
            .auth(self.http.post(format!("{}/agent/runtime/{runtime_id}/heartbeat", self.base)))
            .send()
            .await?
            .status();
        if self.key_revoked(code) {
            anyhow::bail!("API key revoked or invalid");
        }
        Ok(code != reqwest::StatusCode::NOT_FOUND)
    }

    pub async fn claim(
        &self,
        caps: &[String],
        runtime_id: &str,
    ) -> anyhow::Result<Option<WorkSpec>> {
        let caps_csv = caps.join(",");
        let resp = self
            .auth(self.http.get(format!(
                "{}/agent/work/claim?caps={caps_csv}&runtime={runtime_id}",
                self.base
            )))
            .timeout(CLAIM_TIMEOUT)
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }
        let resp = self.ensure_ok(resp)?;
        Ok(Some(resp.json().await?))
    }

    pub async fn post_event(&self, attempt_id: &str, ev: &ExecEvent) {
        let _ = self
            .auth(self.http.post(format!("{}/delivery/{attempt_id}/events", self.base)))
            .json(&json!({"kind": ev.kind, "message": ev.message}))
            .send()
            .await;
    }

    // Terminal reports (complete/fail/design) must retry: a dropped one = lost delivery
    // (server reclaims on timeout if retries are exhausted).
    pub async fn complete(
        &self,
        attempt_id: &str,
        kind: &str,
        reference: &str,
        summary: &str,
    ) -> anyhow::Result<()> {
        retry("complete", REPORT_ATTEMPTS, || async {
            let resp = self
                .auth(self.http.post(format!("{}/delivery/{attempt_id}/complete", self.base)))
                .json(&json!({"kind": kind, "reference": reference, "summary": summary}))
                .send()
                .await?;
            self.ensure_ok(resp)?;
            Ok(())
        })
        .await
    }

    pub async fn fail(&self, attempt_id: &str, error: &str) -> anyhow::Result<()> {
        retry("fail", REPORT_ATTEMPTS, || async {
            let resp = self
                .auth(self.http.post(format!("{}/delivery/{attempt_id}/fail", self.base)))
                .json(&json!({"error": error}))
                .send()
                .await?;
            self.ensure_ok(resp)?;
            Ok(())
        })
        .await
    }

    pub async fn post_design(&self, proposal_id: &str, doc: &str) -> anyhow::Result<()> {
        retry("post_design", REPORT_ATTEMPTS, || async {
            let resp = self
                .auth(self.http.post(format!("{}/proposal/{proposal_id}/design", self.base)))
                .json(&json!({"doc": doc}))
                .send()
                .await?;
            self.ensure_ok(resp)?;
            Ok(())
        })
        .await
    }
}

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

    fn static_client() -> ServerClient {
        ServerClient::with_api_key("http://127.0.0.1:9", "sak_0123456789abcdef.deadbeef")
            .expect("client")
    }

    #[test]
    fn with_api_key_attaches_bearer_verbatim() {
        let c = static_client();
        let req = c.auth(c.http.get("http://127.0.0.1:9/agent/work/claim")).build().expect("req");
        assert_eq!(
            req.headers()["authorization"],
            "Bearer sak_0123456789abcdef.deadbeef",
            "static key should be attached verbatim as bearer"
        );
    }

    #[test]
    fn only_401_is_treated_as_key_revoked() {
        let c = static_client();
        assert!(c.key_revoked(reqwest::StatusCode::UNAUTHORIZED));
        // Non-401 is not revocation (403 = insufficient perms, 5xx = server issue; normal retry semantics apply).
        assert!(!c.key_revoked(reqwest::StatusCode::FORBIDDEN));
        assert!(!c.key_revoked(reqwest::StatusCode::INTERNAL_SERVER_ERROR));
    }
}
