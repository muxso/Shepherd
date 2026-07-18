//! Pool-runner wire protocol and remote execution loop.
//!
//! The server compiles a scenario into a self-contained plan (case references
//! already resolved to full request specs) and pushes it over the runner's
//! outbound WebSocket. The runner executes it with the same PlanExecutor the
//! server uses in-process, forwarding per-step events as they happen, so the
//! resulting report is indistinguishable from a local run.

pub mod protocol;

use std::sync::Arc;

use api_test::adapters::local::{CaseResultSink, CaseRunSpec, CaseSpecSource};
use api_test::adapters::plan::{Condition, Leaf, PlanExecutor, PlanNode, StepObserver};
use api_test::domain::ResolvedEnv;
use api_test::ports::PortError;
use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

use protocol::{RunnerMsg, WireEnv, WireNode};

/// Wire plan → executable plan. Every step is already a full request spec.
pub fn to_plan_nodes(nodes: &[WireNode]) -> Vec<PlanNode> {
    nodes.iter().map(to_plan_node).collect()
}

fn to_plan_node(node: &WireNode) -> PlanNode {
    match node {
        WireNode::Step { id, request, assertions, processors } => PlanNode::Leaf(Leaf::Request {
            label: id.clone(),
            request: request.clone(),
            assertions: assertions.clone(),
            processors: processors.clone(),
        }),
        WireNode::Loop { times, body } => {
            PlanNode::Loop { times: *times, body: to_plan_nodes(body) }
        }
        WireNode::If { variable, operator, value, body } => PlanNode::If {
            condition: Condition {
                variable: variable.clone(),
                condition: *operator,
                value: value.clone(),
            },
            body: to_plan_nodes(body),
        },
        WireNode::Once { id, body } => PlanNode::Once { id: *id, body: to_plan_nodes(body) },
        WireNode::Timer { ms } => PlanNode::Timer { ms: *ms },
    }
}

pub fn to_resolved_env(env: &WireEnv) -> ResolvedEnv {
    ResolvedEnv {
        base_url: env.base_url.clone(),
        headers: env.headers.clone(),
        variables: env.variables.clone(),
    }
}

/// Never consulted: the server resolves case references before dispatch.
struct NoSpecs;

#[async_trait]
impl CaseSpecSource for NoSpecs {
    async fn spec_of(&self, _case_id: &str) -> Result<Option<CaseRunSpec>, PortError> {
        Ok(None)
    }
}

/// Streams step results back over the socket instead of writing to PG.
struct ForwardSink {
    tx: UnboundedSender<RunnerMsg>,
}

#[async_trait]
impl CaseResultSink for ForwardSink {
    async fn record(
        &self,
        report_id: &str,
        case_id: &str,
        outcome: &str,
        failures: &[String],
    ) -> Result<(), PortError> {
        let _ = self.tx.send(RunnerMsg::StepFinished {
            run_id: report_id.to_string(),
            step_id: case_id.to_string(),
            outcome: outcome.to_string(),
            failures: failures.to_vec(),
        });
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_detail(
        &self,
        report_id: &str,
        case_id: &str,
        status_code: i32,
        latency_ms: i64,
        resp_size: i64,
        body: &str,
        headers: &[(String, String)],
        assertions: &serde_json::Value,
        extractions: &serde_json::Value,
        req_method: &str,
        req_url: &str,
        req_headers: &[(String, String)],
        req_body: Option<&str>,
        timings: &serde_json::Value,
    ) -> Result<(), PortError> {
        let _ = self.tx.send(RunnerMsg::StepDetail(Box::new(protocol::StepDetail {
            run_id: report_id.to_string(),
            step_id: case_id.to_string(),
            status_code,
            latency_ms,
            resp_size,
            body: body.to_string(),
            headers: headers.to_vec(),
            assertions: assertions.clone(),
            extractions: extractions.clone(),
            req_method: req_method.to_string(),
            req_url: req_url.to_string(),
            req_headers: req_headers.to_vec(),
            req_body: req_body.map(str::to_string),
            timings: timings.clone(),
        })));
        Ok(())
    }
}

struct ForwardObserver {
    tx: UnboundedSender<RunnerMsg>,
}

impl StepObserver for ForwardObserver {
    fn step_started(&self, report_id: &str, step_id: &str) {
        let _ = self.tx.send(RunnerMsg::StepStarted {
            run_id: report_id.to_string(),
            step_id: step_id.to_string(),
        });
    }

    fn step_finished(
        &self,
        _report_id: &str,
        _step_id: &str,
        _outcome: &str,
        _latency: Option<u64>,
    ) {
        // Result sink already forwards the finished event with failure details.
    }
}

/// Executes one dispatched run and streams events; always terminates the run
/// with RunComplete or RunError.
pub async fn execute_assignment(
    run_id: String,
    nodes: Vec<WireNode>,
    env: WireEnv,
    stop_on_failure: bool,
    tx: UnboundedSender<RunnerMsg>,
) {
    let plan = to_plan_nodes(&nodes);
    let resolved = to_resolved_env(&env);
    let executor = PlanExecutor::new(Arc::new(NoSpecs), Arc::new(ForwardSink { tx: tx.clone() }))
        .with_observer(Arc::new(ForwardObserver { tx: tx.clone() }));
    match executor.run(&run_id, &plan, &resolved, stop_on_failure).await {
        Ok(all_pass) => {
            let _ = tx.send(RunnerMsg::RunComplete { run_id, all_pass });
        }
        Err(e) => {
            let _ = tx.send(RunnerMsg::RunError { run_id, message: format!("{e:?}") });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_runner::{Assertion, HttpMethod, MatchCondition, RequestSpec};

    fn step(id: &str, url: &str) -> WireNode {
        WireNode::Step {
            id: id.into(),
            request: RequestSpec {
                method: HttpMethod::Get,
                url: url.into(),
                headers: vec![],
                body: None,
            },
            assertions: vec![Assertion::StatusIs(200)],
            processors: vec![],
        }
    }

    #[test]
    fn wire_roundtrip_preserves_structure() {
        let nodes = vec![
            step("a", "http://x/ok"),
            WireNode::Loop { times: 2, body: vec![step("b", "http://x/b")] },
            WireNode::If {
                variable: "go".into(),
                operator: MatchCondition::Equals,
                value: "yes".into(),
                body: vec![step("c", "http://x/c")],
            },
            WireNode::Once { id: 1, body: vec![WireNode::Timer { ms: 5 }] },
        ];
        let json = serde_json::to_string(&nodes).expect("ser");
        let back: Vec<WireNode> = serde_json::from_str(&json).expect("de");
        assert_eq!(back, nodes);
    }

    #[test]
    fn wire_to_plan_maps_all_variants() {
        let nodes = vec![
            step("a", "http://x/ok"),
            WireNode::Loop { times: 3, body: vec![step("b", "http://x/b")] },
            WireNode::Timer { ms: 7 },
        ];
        let plan = to_plan_nodes(&nodes);
        assert!(matches!(&plan[0], PlanNode::Leaf(Leaf::Request { label, .. }) if label == "a"));
        assert!(matches!(&plan[1], PlanNode::Loop { times: 3, .. }));
        assert!(matches!(&plan[2], PlanNode::Timer { ms: 7 }));
    }

    #[tokio::test]
    async fn execute_assignment_streams_events_and_completes() {
        use axum_liteish::spawn_ok_server;
        let base = spawn_ok_server().await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        execute_assignment(
            "r1".into(),
            vec![step("s1", &format!("{base}/ok"))],
            WireEnv::default(),
            false,
            tx,
        )
        .await;
        let mut kinds = Vec::new();
        while let Ok(m) = rx.try_recv() {
            kinds.push(match m {
                RunnerMsg::StepStarted { .. } => "started",
                RunnerMsg::StepFinished { .. } => "finished",
                RunnerMsg::StepDetail(_) => "detail",
                RunnerMsg::RunComplete { all_pass, .. } => {
                    assert!(all_pass);
                    "complete"
                }
                _ => "other",
            });
        }
        assert_eq!(kinds, vec!["started", "finished", "detail", "complete"]);
    }

    // Minimal in-test HTTP endpoint without pulling axum into deps: raw tokio TCP.
    mod axum_liteish {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        pub async fn spawn_ok_server() -> String {
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
            let addr = listener.local_addr().expect("addr");
            tokio::spawn(async move {
                loop {
                    let Ok((mut sock, _)) = listener.accept().await else { break };
                    tokio::spawn(async move {
                        let mut buf = [0u8; 1024];
                        let _ = sock.read(&mut buf).await;
                        let body = "{\"ok\":true}";
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = sock.write_all(resp.as_bytes()).await;
                    });
                }
            });
            format!("http://{addr}")
        }
    }
}
