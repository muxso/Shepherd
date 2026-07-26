use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ApiCmd {
    /// Batch-run API cases.
    BatchRun {
        #[arg(long)]
        project: String,
        #[arg(long, value_delimiter = ',')]
        cases: Vec<String>,
        /// Run mode (e.g. SERIAL | PARALLEL).
        #[arg(long = "mode", default_value = "PARALLEL")]
        run_mode: String,
        /// Resource pool id (batch runs require it, client-provided).
        #[arg(long)]
        pool: Option<String>,
        /// Environment id for the run (injects base_url/default headers/variables).
        #[arg(long)]
        env: Option<String>,
    },
}

pub fn run(cmd: ApiCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        ApiCmd::BatchRun {
            project,
            cases,
            run_mode,
            pool,
            env,
        } => pretty(&c.post(
            "/api/batch-run",
            json!({"projectId": project, "caseIds": cases, "runMode": run_mode, "poolId": pool, "environmentId": env}),
            true,
        )?),
    };
    Ok(())
}
