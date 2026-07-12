use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use orchestrator::judges::AcceptAllJudge;
use orchestrator::ports::{DeliverableView, Judge, Verdict};

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
                Err(e) => {
                    Verdict { passed: false, reason: format!("judge 响应解析失败: {e}") }
                }
            },
            Ok(r) => Verdict { passed: false, reason: format!("judge HTTP {}", r.status()) },
            Err(e) => Verdict { passed: false, reason: format!("judge 不可达: {e}") },
        }
    }
}

pub fn build_judge() -> Arc<dyn Judge> {
    if let Some(j) = crate::llm::judge() {
        return j;
    }
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
