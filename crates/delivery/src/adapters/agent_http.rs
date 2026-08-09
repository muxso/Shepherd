use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::domain::{Deliverable, DeliverableKind, EventKind, NewExecutionEvent};
use crate::ports::{AgentExecutor, DispatchOutcome, EventSink, ExecError, WorkSpec};

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
    instructions: Option<&'a str>,
}

#[derive(Deserialize)]
struct DeliverableDto {
    kind: String,
    reference: String,
    #[serde(default)]
    summary: String,
}

#[derive(Deserialize)]
struct EventDto {
    kind: String,
    message: String,
    #[serde(default)]
    detail: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DispatchResponse {
    status: String,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    deliverable: Option<DeliverableDto>,
    #[serde(default)]
    events: Vec<EventDto>,
}

#[async_trait]
impl AgentExecutor for HttpAgentExecutor {
    async fn dispatch(
        &self,
        spec: &WorkSpec,
        sink: &dyn EventSink,
    ) -> Result<DispatchOutcome, ExecError> {
        let dto = WorkSpecDto {
            decomposition_id: &spec.decomposition_id,
            task_id: &spec.task_id,
            title: &spec.title,
            description: &spec.description,
            acceptance_criteria: &spec.acceptance_criteria,
            executor: spec.executor.as_str(),
            context: spec.context.as_deref(),
            instructions: spec.instructions.as_deref(),
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

        for ev in &body.events {
            let kind = EventKind::parse(&ev.kind).unwrap_or(EventKind::Log);
            if let Ok(e) = NewExecutionEvent::new(kind, &ev.message, ev.detail.as_deref()) {
                sink.emit(e).await;
            }
        }

        match body.status.as_str() {
            "accepted" => {
                let run_id = body
                    .run_id
                    .ok_or_else(|| ExecError::Backend("accepted missing runId".into()))?;
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
    use crate::ports::NoopEventSink;
    use axum::{routing::post, Json, Router};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        kinds: Mutex<Vec<String>>,
    }
    #[async_trait]
    impl EventSink for RecordingSink {
        async fn emit(&self, e: NewExecutionEvent) {
            self.kinds
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(e.kind.as_str().to_string());
        }
    }

    fn spec() -> WorkSpec {
        WorkSpec {
            attempt_id: "a1".into(),
            decomposition_id: "d1".into(),
            task_id: "t1".into(),
            title: "build".into(),
            description: "".into(),
            acceptance_criteria: vec![],
            executor: ExecutorKind::Codex,
            context: None,
            instructions: None,
            target_runtime: None,
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
        match HttpAgentExecutor::new(url).dispatch(&spec(), &NoopEventSink).await.expect("dispatch")
        {
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
        match HttpAgentExecutor::new(url).dispatch(&spec(), &NoopEventSink).await.expect("dispatch")
        {
            DispatchOutcome::Completed { deliverable } => {
                assert_eq!(deliverable.kind, DeliverableKind::PullRequest);
                assert_eq!(deliverable.reference, "pr/9");
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn emits_events_returned_by_remote() {
        let app = Router::new().route(
            "/dispatch",
            post(|| async {
                Json(serde_json::json!({
                    "status":"completed",
                    "deliverable":{"kind":"DIFF","reference":"branch:y","summary":"ok"},
                    "events":[
                        {"kind":"DECISION","message":"using argon2","detail":"PHC"},
                        {"kind":"TEST_RESULT","message":"12/12 passed"}
                    ]
                }))
            }),
        );
        let url = serve(app).await;
        let sink = RecordingSink::default();
        let out = HttpAgentExecutor::new(url).dispatch(&spec(), &sink).await.expect("dispatch");
        assert!(matches!(out, DispatchOutcome::Completed { .. }));
        assert_eq!(
            sink.kinds.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_slice(),
            &["DECISION".to_string(), "TEST_RESULT".to_string()]
        );
    }

    #[tokio::test]
    async fn non_2xx_is_error() {
        let app = Router::new().route(
            "/dispatch",
            post(|| async { (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
        );
        let url = serve(app).await;
        assert!(matches!(
            HttpAgentExecutor::new(url).dispatch(&spec(), &NoopEventSink).await,
            Err(ExecError::Backend(_))
        ));
    }
}
