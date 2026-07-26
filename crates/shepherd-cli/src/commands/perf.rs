use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum PerfCmd {
    /// Start a load test round (runs in the background, returns reportId immediately).
    Run {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GET")]
        method: String,
        /// Number of concurrent "virtual users".
        #[arg(long, default_value_t = 10)]
        concurrency: u32,
        /// Total request count (ignored in duration mode).
        #[arg(long, default_value_t = 100)]
        iterations: u32,
        /// Duration mode: keep the load running for this many ms (overrides --iterations when set).
        #[arg(long = "duration-ms")]
        duration_ms: Option<u64>,
        /// Assertion: expected status code (HTTP = status code; 0 means OK for other protocols).
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
        /// Assertion: output contains substring.
        #[arg(long)]
        contains: Option<String>,
        /// Assertion: output equals.
        #[arg(long)]
        equals: Option<String>,
        /// Assertion: per-request latency must not exceed this many ms.
        #[arg(long = "latency-under")]
        latency_under: Option<u64>,
        /// Protocol: HTTP (default) | SQL | GRPC | REDIS | MYSQL | WEBSOCKET. For non-HTTP, --url is the target and --query the payload.
        #[arg(long, default_value = "HTTP")]
        protocol: String,
        /// Protocol payload: SQL = statement, GRPC = method path, REDIS = command, WS = message.
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value = "")]
        project: String,
    },
    /// Fetch a load test report (throughput/error rate/latency percentiles).
    Report {
        #[arg(long)]
        id: String,
    },
}

pub fn run(cmd: PerfCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        PerfCmd::Run {
            url,
            method,
            concurrency,
            iterations,
            duration_ms,
            expect_status,
            contains,
            equals,
            latency_under,
            protocol,
            query,
            project,
        } => pretty(&c.post(
            "/perf/run",
            json!({"url": url, "method": method, "concurrency": concurrency,
                   "iterations": iterations, "durationMs": duration_ms,
                   "expectStatus": expect_status, "expectContains": contains,
                   "expectEquals": equals, "latencyUnderMs": latency_under,
                   "protocol": protocol, "query": query,
                   "projectId": project}),
            true,
        )?),
        PerfCmd::Report { id } => pretty(&c.get(&format!("/perf/report/{id}"), true)?),
    };
    Ok(())
}
