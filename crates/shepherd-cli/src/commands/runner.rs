use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum RunnerCmd {
    /// Register a runner-agent for an environment.
    Register {
        #[arg(long)]
        name: String,
        /// Agent endpoint, e.g. http://10.0.0.5:9100.
        #[arg(long = "base-url")]
        base_url: String,
        /// Optional shared secret (matches the agent's RUNNER_TOKEN).
        #[arg(long)]
        token: Option<String>,
    },
    /// List registered agents.
    List,
    /// Dispatch a self-contained case to an agent for in-place execution.
    Run {
        /// Agent id (from register/list).
        #[arg(long)]
        agent: String,
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long)]
        body: Option<String>,
        /// Expected status code (generates a StatusIs assertion when set).
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
    },
    /// Dispatch a **stored case** (api-case) to an agent.
    RunCase {
        #[arg(long)]
        agent: String,
        #[arg(long)]
        case: String,
    },
    /// List an agent's remote execution history.
    Executions {
        #[arg(long)]
        agent: String,
    },
    /// Re-fetch an agent's /protocols and refresh its protocol capability snapshot.
    Refresh {
        #[arg(long)]
        agent: String,
    },
    /// Probe by protocol: given only a protocol, the server picks a supporting agent for in-place execution (assertions supported).
    Probe {
        /// Protocol name (http/grpc/sql/…).
        #[arg(long)]
        protocol: String,
        /// Target (URL / gRPC endpoint / connection string).
        #[arg(long)]
        target: String,
        /// Payload (HTTP body / gRPC request bytes / SQL statement).
        #[arg(long)]
        payload: Option<String>,
        /// Extra protocol params k=v (repeatable; e.g. method=POST, gRPC method=/pkg.Svc/M).
        #[arg(long = "meta")]
        meta: Vec<String>,
        /// Assertion: status code equals.
        #[arg(long = "expect-status")]
        expect_status: Option<i64>,
        /// Assertion: output contains substring.
        #[arg(long)]
        contains: Option<String>,
        /// Assertion: latency under N ms.
        #[arg(long = "latency-under")]
        latency_under: Option<u64>,
    },
}

pub fn run(cmd: RunnerCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        RunnerCmd::Register { name, base_url, token } => pretty(&c.post(
            "/runner-agent",
            json!({"name": name, "baseUrl": base_url, "token": token}),
            true,
        )?),
        RunnerCmd::List => pretty(&c.get("/runner-agent", true)?),
        RunnerCmd::Run {
            agent,
            url,
            method,
            body,
            expect_status,
        } => pretty(&c.post(
            &format!("/runner-agent/{agent}/run"),
            json!({
                "request": {"method": method.to_uppercase(), "url": url, "headers": [], "body": body},
                "assertions": status_assertions(expect_status),
            }),
            true,
        )?),
        RunnerCmd::RunCase { agent, case } => pretty(&c.post(
            &format!("/runner-agent/{agent}/run-case"),
            json!({"caseId": case}),
            true,
        )?),
        RunnerCmd::Executions { agent } => {
            pretty(&c.get(&format!("/runner-agent/{agent}/executions"), true)?)
        }
        RunnerCmd::Refresh { agent } => pretty(&c.post(
            &format!("/runner-agent/{agent}/refresh"),
            json!({}),
            true,
        )?),
        RunnerCmd::Probe {
            protocol,
            target,
            payload,
            meta,
            expect_status,
            contains,
            latency_under,
        } => {
            let mut metadata = serde_json::Map::new();
            for kv in &meta {
                if let Some((k, v)) = kv.split_once('=') {
                    metadata.insert(k.to_string(), json!(v));
                }
            }
            let mut assertions: Vec<Value> = Vec::new();
            if let Some(s) = expect_status {
                assertions.push(json!({"type": "status_is", "value": s}));
            }
            if let Some(sub) = &contains {
                assertions.push(json!({"type": "output_contains", "value": sub}));
            }
            if let Some(ms) = latency_under {
                assertions.push(json!({"type": "latency_under_ms", "value": ms}));
            }
            pretty(&c.post(
                "/runner/probe",
                json!({
                    "protocol": protocol, "target": target, "payload": payload,
                    "metadata": metadata, "assertions": assertions,
                }),
                true,
            )?)
        }
    };
    Ok(())
}
