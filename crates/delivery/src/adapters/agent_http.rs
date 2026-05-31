//! 远端 Agent HTTP 执行者(feature = "exec-http"):POST 工作规格给 Agent SDK/API 服务。
//!
//! 服务响应约定:
//! - `{"status":"accepted","runId":"..."}`            → 异步接单 → `DispatchOutcome::Accepted`;
//! - `{"status":"completed","deliverable":{...}}`     → 同步完成 → `DispatchOutcome::Completed`。
//!
//! client 默认 `no_proxy`(agent 端点通常内网/本地,避免被全局代理劫持)。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::domain::{Deliverable, DeliverableKind};
use crate::ports::{AgentExecutor, DispatchOutcome, ExecError, WorkSpec};

#[derive(Clone)]
pub struct HttpAgentExecutor {
    client: reqwest::Client,
    base_url: String,
}

impl HttpAgentExecutor {
    pub fn new(base_url: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, base_url: base_url.into() }
    }
}

// 端口类型保持 serde-free;此处用本地 DTO 序列化(camelCase 对接外部服务)。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkSpecDto<'a> {
    decomposition_id: &'a str,
    task_id: &'a str,
    title: &'a str,
    description: &'a str,
    acceptance_criteria: &'a [String],
    executor: &'a str,
    context: Option<&'a str>,
}

#[derive(Deserialize)]
struct DeliverableDto {
    kind: String,
    reference: String,
    #[serde(default)]
    summary: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DispatchResponse {
    status: String,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    deliverable: Option<DeliverableDto>,
}

#[async_trait]
impl AgentExecutor for HttpAgentExecutor {
    async fn dispatch(&self, spec: &WorkSpec) -> Result<DispatchOutcome, ExecError> {
        let dto = WorkSpecDto {
            decomposition_id: &spec.decomposition_id,
            task_id: &spec.task_id,
            title: &spec.title,
            description: &spec.description,
            acceptance_criteria: &spec.acceptance_criteria,
            executor: spec.executor.as_str(),
            context: spec.context.as_deref(),
        };
        let url = format!("{}/dispatch", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .json(&dto)
            .send()
            .await
            .map_err(|e| ExecError::Backend(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ExecError::Backend(format!("agent http status {}", resp.status())));
        }
        let body: DispatchResponse =
            resp.json().await.map_err(|e| ExecError::Backend(e.to_string()))?;

        match body.status.as_str() {
            "accepted" => {
                let run_id =
                    body.run_id.ok_or_else(|| ExecError::Backend("accepted missing runId".into()))?;
                Ok(DispatchOutcome::Accepted { run_id })
            }
            "completed" => {
                let d = body
                    .deliverable
                    .ok_or_else(|| ExecError::Backend("completed missing deliverable".into()))?;
                let kind = DeliverableKind::parse(&d.kind).unwrap_or(DeliverableKind::Diff);
                Ok(DispatchOutcome::Completed {
                    deliverable: Deliverable { kind, reference: d.reference, summary: d.summary },
                })
            }
            other => Err(ExecError::Backend(format!("unknown agent status: {other}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ExecutorKind;
    use axum::{routing::post, Json, Router};

    fn spec() -> WorkSpec {
        WorkSpec {
            decomposition_id: "d1".into(),
            task_id: "t1".into(),
            title: "build".into(),
            description: "".into(),
            acceptance_criteria: vec![],
            executor: ExecutorKind::Codex,
            context: None,
        }
    }

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn parses_accepted() {
        let app = Router::new().route(
            "/dispatch",
            post(|| async { Json(serde_json::json!({"status":"accepted","runId":"run-x"})) }),
        );
        let url = serve(app).await;
        match HttpAgentExecutor::new(url).dispatch(&spec()).await.expect("dispatch") {
            DispatchOutcome::Accepted { run_id } => assert_eq!(run_id, "run-x"),
            other => panic!("expected Accepted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parses_completed() {
        let app = Router::new().route(
            "/dispatch",
            post(|| async {
                Json(serde_json::json!({
                    "status":"completed",
                    "deliverable":{"kind":"PULL_REQUEST","reference":"pr/9","summary":"done"}
                }))
            }),
        );
        let url = serve(app).await;
        match HttpAgentExecutor::new(url).dispatch(&spec()).await.expect("dispatch") {
            DispatchOutcome::Completed { deliverable } => {
                assert_eq!(deliverable.kind, DeliverableKind::PullRequest);
                assert_eq!(deliverable.reference, "pr/9");
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn non_2xx_is_error() {
        let app = Router::new().route(
            "/dispatch",
            post(|| async { (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
        );
        let url = serve(app).await;
        assert!(matches!(
            HttpAgentExecutor::new(url).dispatch(&spec()).await,
            Err(ExecError::Backend(_))
        ));
    }
}
