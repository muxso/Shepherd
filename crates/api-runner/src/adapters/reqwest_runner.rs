//! reqwest 执行器:把 `RequestSpec` 真发出去,得到 `ResponseSnapshot`,
//! 再交给纯函数 `evaluate` 判定。这一段是唯一碰 IO 的地方。

use crate::domain::{evaluate, Assertion, CaseReport, RequestSpec, ResponseSnapshot};

#[derive(Debug)]
pub enum RunError {
    Transport(String),
}

#[derive(Clone, Default)]
pub struct ReqwestRunner {
    client: reqwest::Client,
}

impl ReqwestRunner {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }

    /// 注入自定义 client(超时、代理策略、连接池等)。
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// 绕过环境代理(http_proxy/all_proxy 等)的 client。就地 runner 直连被测主机,
    /// 不应被开发环境的全局代理劫持(与 CLI、本地 e2e 的 no_proxy 约定一致)。
    pub fn no_proxy() -> Self {
        let client = reqwest::Client::builder().no_proxy().build().expect("build no_proxy client");
        Self { client }
    }

    /// 执行一次请求,返回响应快照。
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
        // 计时覆盖「发送 → 读完响应体」,供 RESPONSE_TIME 断言。
        let started = std::time::Instant::now();
        let resp = req.send().await.map_err(|e| RunError::Transport(e.to_string()))?;

        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = resp.text().await.map_err(|e| RunError::Transport(e.to_string()))?;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(ResponseSnapshot { status, headers, body, elapsed_ms })
    }

    /// 执行 + 断言,一步得到用例结果。传输失败也算 Error(带原因)。
    pub async fn run_case(&self, spec: &RequestSpec, assertions: &[Assertion]) -> CaseReport {
        self.run_case_with_snapshot(spec, assertions).await.0
    }

    /// 同 [`run_case`](Self::run_case),但额外返回响应快照(供后置 EXTRACT 提取参数)。
    /// 传输失败时快照为 `None`。
    pub async fn run_case_with_snapshot(
        &self,
        spec: &RequestSpec,
        assertions: &[Assertion],
    ) -> (CaseReport, Option<ResponseSnapshot>) {
        match self.execute(spec).await {
            Ok(snapshot) => {
                let report = evaluate(assertions, &snapshot);
                (report, Some(snapshot))
            }
            Err(RunError::Transport(msg)) => (
                CaseReport {
                    outcome: crate::domain::CaseOutcome::Error,
                    failures: vec![format!("transport: {msg}")],
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
        // no_proxy:绕过环境里的 http_proxy,确保真的命中"连不上"的传输失败
        let client = reqwest::Client::builder().no_proxy().build().expect("client");
        let runner = ReqwestRunner::with_client(client);
        let report = runner
            .run_case(&get_spec("http://127.0.0.1:1/nope".into()), &[Assertion::StatusIs(200)])
            .await;
        assert_eq!(report.outcome, CaseOutcome::Error);
        assert!(report.failures[0].contains("transport"));
    }
}
