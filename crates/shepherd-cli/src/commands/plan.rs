use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum PlanCmd {
    /// Create a test plan (or group).
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// TEST_PLAN | GROUP.
        #[arg(long = "type", default_value = "TEST_PLAN")]
        plan_type: String,
        #[arg(long)]
        group: Option<String>,
    },
    /// Plan execution statistics.
    Stats {
        #[arg(long)]
        id: String,
    },
    /// Export a report to stdout: HTML by default (e.g. `> report.html`); --markdown for Markdown (e.g. `> report.md`).
    Report {
        #[arg(long)]
        id: String,
        /// Export Markdown (instead of HTML).
        #[arg(long)]
        markdown: bool,
    },
    /// Set scheduled runs for a plan (6-field cron: sec min hour day month weekday).
    Schedule {
        #[arg(long)]
        id: String,
        /// Cron expression, e.g. "0 */5 * * * *" (every 5 minutes).
        #[arg(long)]
        cron: String,
    },
    /// Fetch a plan's scheduled-run snapshots (pass rate / execution rate trends).
    Runs {
        #[arg(long)]
        id: String,
    },
    /// Attach a case to a plan.
    LinkCase {
        #[arg(long)]
        id: String,
        #[arg(long)]
        case: String,
        #[arg(long, default_value = "")]
        name: String,
    },
    /// Write back a case's execution result within a plan.
    Result {
        #[arg(long)]
        id: String,
        #[arg(long)]
        case: String,
        /// SUCCESS | ERROR | FAKE_ERROR | BLOCK | PENDING.
        #[arg(long, default_value = "SUCCESS")]
        status: String,
        #[arg(long = "latency-ms", default_value_t = 0)]
        latency_ms: u64,
        #[arg(long = "status-code")]
        status_code: Option<i64>,
        #[arg(long)]
        body: Option<String>,
        /// Assertions JSON array, e.g. '[{"item":"status","actual":"200","condition":"equals","expected":"200","passed":true}]'.
        #[arg(long = "assertions-json")]
        assertions_json: Option<String>,
    },
    /// List cases in a plan (with status).
    Cases {
        #[arg(long)]
        id: String,
    },
    /// Run a plan: execute attached cases and auto write back results (so a later `plan report` shows real data).
    Run {
        #[arg(long)]
        id: String,
    },
}

pub fn run(cmd: PlanCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        PlanCmd::Create { project, name, plan_type, group } => {
            let mut body = json!({"projectId": project, "name": name, "type": plan_type});
            if let Some(g) = group {
                body["groupId"] = json!(g);
            }
            pretty(&c.post("/test-plan", body, true)?)
        }
        PlanCmd::Stats { id } => pretty(&c.get(&format!("/test-plan/{id}/statistics"), true)?),
        PlanCmd::Report { id, markdown } => {
            let path = if markdown {
                format!("/test-plan/{id}/report.md")
            } else {
                format!("/test-plan/{id}/report")
            };
            print!("{}", c.get_text(&path, false)?)
        }
        PlanCmd::Schedule { id, cron } => {
            pretty(&c.post(&format!("/test-plan/{id}/schedule"), json!({"cron": cron}), true)?)
        }
        PlanCmd::Runs { id } => pretty(&c.get(&format!("/test-plan/{id}/runs"), true)?),
        PlanCmd::LinkCase { id, case, name } => pretty(&c.post(
            &format!("/test-plan/{id}/cases"),
            json!({"caseId": case, "name": name}),
            true,
        )?),
        PlanCmd::Result { id, case, status, latency_ms, status_code, body, assertions_json } => {
            let assertions: Value = match assertions_json {
                Some(aj) => serde_json::from_str(&aj)
                    .map_err(|e| format!("--assertions-json 不是合法 JSON: {e}"))?,
                None => json!([]),
            };
            pretty(&c.post(
                &format!("/test-plan/{id}/cases/{case}/result"),
                json!({"status": status, "latencyMs": latency_ms, "statusCode": status_code,
                       "body": body, "assertions": assertions}),
                true,
            )?)
        }
        PlanCmd::Cases { id } => pretty(&c.get(&format!("/test-plan/{id}/cases"), true)?),
        PlanCmd::Run { id } => pretty(&c.post(&format!("/test-plan/{id}/run"), json!({}), true)?),
    };
    Ok(())
}
