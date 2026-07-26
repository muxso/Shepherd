use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum DecompositionCmd {
    /// Fetch the full decomposition graph (complete / readyTaskIds / each task's current status).
    Get {
        #[arg(long)]
        id: String,
    },
    /// Show only tasks currently ready (all dependencies Verified, dispatchable).
    Ready {
        #[arg(long)]
        id: String,
    },
    /// Parallel orchestration: dispatch the whole task DAG layer by layer along dependencies (auto-drives verification gates).
    Run {
        #[arg(long)]
        id: String,
        #[arg(long, default_value = "CLAUDE_CODE")]
        executor: String,
        #[arg(long, default_value_t = 4)]
        concurrency: usize,
    },
}

pub fn run(cmd: DecompositionCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        DecompositionCmd::Get { id } => pretty(&c.get(&format!("/decomposition/{id}"), true)?),
        DecompositionCmd::Ready { id } => {
            pretty(&c.get(&format!("/decomposition/{id}/ready"), true)?)
        }
        DecompositionCmd::Run { id, executor, concurrency } => pretty(&c.post(
            &format!("/decomposition/{id}/run"),
            json!({"executor": executor, "maxConcurrency": concurrency}),
            true,
        )?),
    };
    Ok(())
}
