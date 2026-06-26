use std::time::Duration;

use crate::domain::{Assertion, CaseReport, RequestSpec, ResponseSnapshot};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum RunError {
    Timeout(String),
    Connect(String),
    Transport(String),
}

impl RunError {
    fn from_reqwest(e: &reqwest::Error) -> Self {
        if e.is_timeout() {
            RunError::Timeout(e.to_string())
        } else if e.is_connect() {
            RunError::Connect(e.to_string())
        } else {
            RunError::Transport(e.to_string())
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            RunError::Timeout(_) => "timeout",
            RunError::Connect(_) => "connect",
            RunError::Transport(_) => "transport",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            RunError::Timeout(m) | RunError::Connect(m) | RunError::Transport(m) => m,
        }
    }
}

fn configured_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().timeout(DEFAULT_TIMEOUT).connect_timeout(DEFAULT_CONNECT_TIMEOUT)
}

#[derive(Clone)]
pub struct ReqwestRunner {
    client: reqwest::Client,
}

impl Default for ReqwestRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl ReqwestRunner {
    pub fn new() -> Self {
        Self { client: configured_builder().build().unwrap_or_default() }
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub fn no_proxy() -> Self {
        Self { client: configured_builder().no_proxy().build().unwrap_or_default() }
    }

    pub async fn execute(&self, spec: &RequestSpec) -> Result<ResponseSnapshot, RunError> {
        let method = reqwest::Method::from_bytes(spec.method.as_str().as_bytes())
            .map_err(|e| RunError::Transport(e.to_string()))?;
        let mut req = self.client.request(method, &spec.url);
        for (k, v) in &spec.headers {
            req = req.header(k, v);
        }
        if let Some(body) = &spec.body {
            req = req.body(body.clone());
        }
        let started = std::time::Instant::now();
        let resp = req.send().await.map_err(|e| RunError::from_reqwest(&e))?;

        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = resp.text().await.map_err(|e| RunError::from_reqwest(&e))?;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(ResponseSnapshot { status, headers, body, elapsed_ms })
    }

    pub async fn run_case(&self, spec: &RequestSpec, assertions: &[Assertion]) -> CaseReport {
        self.run_case_with_snapshot(spec, assertions).await.0
    }

    pub async fn run_case_with_snapshot(
        &self,
        spec: &RequestSpec,
        assertions: &[Assertion],
    ) -> (CaseReport, Option<ResponseSnapshot>) {
        self.run_case_with_snapshot_vars(spec, assertions, &std::collections::BTreeMap::new()).await
    }

    pub async fn run_case_with_snapshot_vars(
        &self,
        spec: &RequestSpec,
        assertions: &[Assertion],
        vars: &std::collections::BTreeMap<String, String>,
    ) -> (CaseReport, Option<ResponseSnapshot>) {
        match self.execute(spec).await {
            Ok(snapshot) => {
                let report = crate::domain::evaluate_with_vars(assertions, &snapshot, vars);
                (report, Some(snapshot))
            }
            Err(e) => (
                CaseReport {
                    outcome: crate::domain::CaseOutcome::Error,
                    failures: vec![format!("transport({}): {}", e.kind(), e.message())],
                },
                None,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CaseOutcome, HttpMethod};
    use axum::{routing::get, Json, Router};
    use tokio::net::TcpListener;

    async fn spawn() -> String {
        let app = Router::new().route(
            "/users/u1",
            get(|| async {
                ([("x-trace", "t-123")], Json(serde_json::json!({"id":"u1","name":"Alice"})))
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    fn get_spec(url: String) -> RequestSpec {
        RequestSpec { method: HttpMethod::Get, url, headers: vec![], body: None }
    }

    #[tokio::test]
    async fn runs_real_request_and_passes_all_assertions() {
        let base = spawn().await;
        let runner = ReqwestRunner::new();
        let report = runner
            .run_case(
                &get_spec(format!("{base}/users/u1")),
                &[
                    Assertion::StatusIs(200),
                    Assertion::JsonFieldEquals { pointer: "/name".into(), expected: "Alice".into() },
                    Assertion::HeaderEquals { name: "x-trace".into(), value: "t-123".into() },
                ],
            )
            .await;
        assert_eq!(report.outcome, CaseOutcome::Success, "failures: {:?}", report.failures);
    }

    #[tokio::test]
    async fn assertion_mismatch_yields_error_with_reasons() {
        let base = spawn().await;
        let runner = ReqwestRunner::new();
        let report = runner
            .run_case(
                &get_spec(format!("{base}/users/u1")),
                &[Assertion::JsonFieldEquals { pointer: "/name".into(), expected: "Bob".into() }],
            )
            .await;
        assert_eq!(report.outcome, CaseOutcome::Error);
        assert_eq!(report.failures.len(), 1);
    }

    #[tokio::test]
    async fn transport_failure_is_an_error_outcome() {
        let client = reqwest::Client::builder().no_proxy().build().expect("client");
        let runner = ReqwestRunner::with_client(client);
        let report = runner
            .run_case(&get_spec("http://127.0.0.1:1/nope".into()), &[Assertion::StatusIs(200)])
            .await;
        assert_eq!(report.outcome, CaseOutcome::Error);
        assert!(report.failures[0].contains("transport"));
        assert!(report.failures[0].contains("connect"), "got: {}", report.failures[0]);
    }

    #[tokio::test]
    async fn slow_target_times_out_with_timeout_category() {
        let app = Router::new().route(
            "/slow",
            get(|| async {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                "late"
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });

        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_millis(50))
            .build()
            .expect("client");
        let runner = ReqwestRunner::with_client(client);
        let report = runner
            .run_case(&get_spec(format!("http://{addr}/slow")), &[Assertion::StatusIs(200)])
            .await;
        assert_eq!(report.outcome, CaseOutcome::Error);
        assert!(report.failures[0].contains("timeout"), "got: {}", report.failures[0]);
    }
}
