use crate::ports::{DispatchOutcome, PortError, RunTask, TaskDispatcher};
use async_trait::async_trait;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskPayload<'a> {
    report_id: &'a str,
    pool_id: &'a str,
    run_mode: &'a str,
    case_ids: &'a [String],
}

#[derive(Clone)]
pub struct HttpTaskDispatcher {
    client: reqwest::Client,
    base_url: String,
}

impl HttpTaskDispatcher {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { client: reqwest::Client::new(), base_url: base_url.into() }
    }

    pub fn with_client(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self { client, base_url: base_url.into() }
    }
}

#[async_trait]
impl TaskDispatcher for HttpTaskDispatcher {
    async fn dispatch_task(&self, task: &RunTask) -> Result<DispatchOutcome, PortError> {
        let url = format!("{}/jmeter/batch-run", self.base_url.trim_end_matches('/'));
        let payload = TaskPayload {
            report_id: &task.report_id,
            pool_id: &task.pool_id,
            run_mode: task.mode.as_str(),
            case_ids: &task.case_ids,
        };
        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| PortError::Backend(format!("dispatch to executor failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(PortError::Backend(format!(
                "executor rejected task: HTTP {}",
                resp.status()
            )));
        }
        Ok(DispatchOutcome::Accepted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::BatchRunMode;
    use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    #[derive(Clone)]
    struct Stub {
        received: Arc<Mutex<Vec<serde_json::Value>>>,
        status: StatusCode,
    }

    async fn handler(State(stub): State<Stub>, Json(body): Json<serde_json::Value>) -> StatusCode {
        stub.received.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(body);
        stub.status
    }

    async fn spawn_stub(status: StatusCode) -> (String, Arc<Mutex<Vec<serde_json::Value>>>) {
        let received = Arc::new(Mutex::new(Vec::new()));
        let stub = Stub { received: received.clone(), status };
        let app = Router::new().route("/jmeter/batch-run", post(handler)).with_state(stub);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });
        (format!("http://{addr}"), received)
    }

    fn task() -> RunTask {
        RunTask {
            report_id: "report-1".into(),
            pool_id: "pool1".into(),
            mode: BatchRunMode::Parallel,
            case_ids: vec!["c1".into(), "c2".into()],
            env: crate::domain::ResolvedEnv::default(),
            environment_id: None,
        }
    }

    #[tokio::test]
    async fn posts_task_payload_to_executor_node() {
        let (base, received) = spawn_stub(StatusCode::OK).await;
        let dispatcher = HttpTaskDispatcher::new(base);

        dispatcher.dispatch_task(&task()).await.expect("dispatch ok");

        let got = received.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(got.len(), 1);
        let body = &got[0];
        assert_eq!(body["reportId"], "report-1");
        assert_eq!(body["poolId"], "pool1");
        assert_eq!(body["runMode"], "PARALLEL");
        assert_eq!(body["caseIds"].as_array().expect("arr").len(), 2);
    }

    #[tokio::test]
    async fn non_2xx_from_node_is_a_dispatch_error() {
        let (base, _) = spawn_stub(StatusCode::SERVICE_UNAVAILABLE).await;
        let dispatcher = HttpTaskDispatcher::new(base);
        let err = dispatcher.dispatch_task(&task()).await;
        assert!(matches!(err, Err(PortError::Backend(_))));
    }

    #[tokio::test]
    async fn unreachable_node_is_a_dispatch_error() {
        let dispatcher = HttpTaskDispatcher::new("http://127.0.0.1:1");
        let err = dispatcher.dispatch_task(&task()).await;
        assert!(matches!(err, Err(PortError::Backend(_))));
    }
}
