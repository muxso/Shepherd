//! JSON messages exchanged on /api/pool-runner/ws. All frames are Text; the
//! `type` tag discriminates. Liveness rides on protocol-level Ping/Pong.

use std::collections::BTreeMap;

use api_runner::{Assertion, MatchCondition, Processor, RequestSpec};
use serde::{Deserialize, Serialize};

/// Environment snapshot resolved server-side (base url, shared headers, variables).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireEnv {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

/// Compiled scenario plan with case references already resolved: the runner
/// needs no database access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WireNode {
    #[serde(rename_all = "camelCase")]
    Step {
        /// Result identity: case id for referenced cases, "METHOD url" label for
        /// inline requests. Matches what the in-process executor records.
        id: String,
        request: RequestSpec,
        #[serde(default)]
        assertions: Vec<Assertion>,
        #[serde(default)]
        processors: Vec<Processor>,
    },
    Loop {
        times: u32,
        body: Vec<WireNode>,
    },
    If {
        variable: String,
        operator: MatchCondition,
        value: String,
        body: Vec<WireNode>,
    },
    Once {
        id: u32,
        body: Vec<WireNode>,
    },
    Timer {
        ms: u64,
    },
}

/// Full result payload of one step, mirroring the local sink's record_detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepDetail {
    pub run_id: String,
    pub step_id: String,
    pub status_code: i32,
    pub latency_ms: i64,
    pub resp_size: i64,
    pub body: String,
    pub headers: Vec<(String, String)>,
    pub assertions: serde_json::Value,
    pub extractions: serde_json::Value,
    pub req_method: String,
    pub req_url: String,
    pub req_headers: Vec<(String, String)>,
    pub req_body: Option<String>,
    pub timings: serde_json::Value,
}

/// Runner → server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RunnerMsg {
    /// First frame after connect; the server registers the runner under poolId.
    #[serde(rename_all = "camelCase")]
    Hello {
        pool_id: String,
        name: String,
        #[serde(default)]
        capabilities: Vec<String>,
        /// Concurrent-run cap the server must respect when assigning work.
        #[serde(default = "default_max_concurrent")]
        max_concurrent: u32,
    },
    #[serde(rename_all = "camelCase")]
    StepStarted { run_id: String, step_id: String },
    #[serde(rename_all = "camelCase")]
    StepFinished {
        run_id: String,
        step_id: String,
        outcome: String,
        #[serde(default)]
        failures: Vec<String>,
    },
    /// Response snapshot for one finished step (absent on transport failure).
    /// Boxed: much larger than the other variants.
    StepDetail(Box<StepDetail>),
    #[serde(rename_all = "camelCase")]
    RunComplete { run_id: String, all_pass: bool },
    #[serde(rename_all = "camelCase")]
    RunError { run_id: String, message: String },
}

pub fn default_max_concurrent() -> u32 {
    4
}

/// Server → runner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerMsg {
    #[serde(rename_all = "camelCase")]
    HelloAck { runner_id: String },
    #[serde(rename_all = "camelCase")]
    Run {
        /// Report id: the runner echoes it on every event.
        run_id: String,
        nodes: Vec<WireNode>,
        env: WireEnv,
        stop_on_failure: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_camel_case() {
        let m = RunnerMsg::StepStarted { run_id: "r".into(), step_id: "s".into() };
        let v = serde_json::to_value(&m).expect("ser");
        assert_eq!(v["type"], "stepStarted");
        assert_eq!(v["runId"], "r");
        let s = serde_json::to_value(ServerMsg::Run {
            run_id: "r".into(),
            nodes: vec![WireNode::Timer { ms: 1 }],
            env: WireEnv::default(),
            stop_on_failure: true,
        })
        .expect("ser");
        assert_eq!(s["type"], "run");
        assert_eq!(s["nodes"][0]["kind"], "timer");
        assert_eq!(s["stopOnFailure"], true);
    }

    #[test]
    fn hello_without_cap_defaults_to_four() {
        let m: RunnerMsg =
            serde_json::from_str(r#"{"type":"hello","poolId":"p","name":"n"}"#).expect("de");
        match m {
            RunnerMsg::Hello { max_concurrent, .. } => assert_eq!(max_concurrent, 4),
            other => panic!("expected hello, got {other:?}"),
        }
    }
}
