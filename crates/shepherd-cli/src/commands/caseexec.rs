use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum CaseExecCmd {
    /// Execution summary for a project (executions / passed / executed cases).
    Summary {
        #[arg(long)]
        project: String,
    },
    /// Daily execution trend (pass/fail over the last N days).
    Trend {
        #[arg(long)]
        project: String,
        /// Lookback window in days (1-90, default 7).
        #[arg(long, default_value_t = 7)]
        days: i32,
    },
}

pub fn run(cmd: CaseExecCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        CaseExecCmd::Summary { project } => {
            pretty(&c.get(&format!("/api/case-exec-summary?projectId={project}"), true)?)
        }
        CaseExecCmd::Trend { project, days } => {
            pretty(&c.get(&format!("/api/exec-trend?projectId={project}&days={days}"), true)?)
        }
    };
    Ok(())
}
