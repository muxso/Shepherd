//! 验证门(judge)选择与 HTTP/LLM 适配器。
//! 默认 `AcceptAllJudge`(保持"交付即通过");设 `SHEPHERD_JUDGE_URL` 则用 `HttpJudge`
//! (把验收标准 + 交付物 POST 给 judge 服务,可由 LLM 背书)。judge 不可达/出错 → **不通过**(fail-closed)。

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use orchestrator::judges::AcceptAllJudge;
use orchestrator::ports::{DeliverableView, Judge, Verdict};

/// 远端 judge(LLM 背书):POST {criteria, deliverable} → {passed, reason}。
struct HttpJudge {
    client: reqwest::Client,
    url: String,
}

#[derive(Deserialize)]
struct JudgeResponse {
    passed: bool,
    #[serde(default)]
    reason: String,
}

#[async_trait]
impl Judge for HttpJudge {
    async fn judge(&self, criteria: &[String], deliverable: &DeliverableView) -> Verdict {
        let body = json!({
            "criteria": criteria,
            "deliverable": { "kind": deliverable.kind, "reference": deliverable.reference, "summary": deliverable.summary }
        });
        match self.client.post(&self.url).json(&body).send().await {
            Ok(r) if r.status().is_success() => match r.json::<JudgeResponse>().await {
                Ok(jr) => Verdict { passed: jr.passed, reason: jr.reason },
                Err(e) => Verdict { passed: false, reason: format!("judge 响应解析失败: {e}") },
            },
            Ok(r) => Verdict { passed: false, reason: format!("judge HTTP {}", r.status()) },
            Err(e) => Verdict { passed: false, reason: format!("judge 不可达: {e}") },
        }
    }
}

/// 按环境选择验证门。
pub fn build_judge() -> Arc<dyn Judge> {
    match std::env::var("SHEPHERD_JUDGE_URL") {
        Ok(url) if !url.trim().is_empty() => {
            let client = reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap_or_else(|_| reqwest::Client::new());
            Arc::new(HttpJudge { client, url })
        }
        _ => Arc::new(AcceptAllJudge),
    }
}
