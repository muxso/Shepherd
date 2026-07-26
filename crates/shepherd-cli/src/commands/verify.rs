use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum VerifyCmd {
    /// Open a verification (with acceptance criteria).
    Create {
        #[arg(long)]
        req: String,
        #[arg(long, default_value_t = 1)]
        version: u32,
        #[arg(long, value_delimiter = ',')]
        criteria: Vec<String>,
    },
    /// Create a coverage link: a task covers an acceptance criterion (requirement -> task traceability).
    Link {
        /// Verification id.
        #[arg(long)]
        id: String,
        /// Acceptance criterion index (0-based).
        #[arg(long)]
        criterion: u32,
        /// Decomposition id.
        #[arg(long)]
        decomp: String,
        /// Task local id (e.g. t1).
        #[arg(long)]
        task: String,
    },
    /// Sync a task's delivery/verification status to its coverage links (task -> implementation traceability).
    Sync {
        #[arg(long)]
        id: String,
        #[arg(long)]
        decomp: String,
        #[arg(long)]
        task: String,
        /// Mark as unverified (default syncs as verified, satisfied=true).
        #[arg(long, default_value_t = false)]
        unsatisfied: bool,
    },
    /// Completeness report.
    Report {
        #[arg(long)]
        id: String,
    },
}

pub fn run(cmd: VerifyCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        VerifyCmd::Create { req, version, criteria } => pretty(&c.post(
            "/verification",
            json!({"requirementId": req, "requirementVersion": version, "criteria": criteria}),
            true,
        )?),
        VerifyCmd::Link { id, criterion, decomp, task } => pretty(&c.post(
            &format!("/verification/{id}/link"),
            json!({"criterionIndex": criterion, "decompositionId": decomp, "taskId": task}),
            true,
        )?),
        VerifyCmd::Sync { id, decomp, task, unsatisfied } => pretty(&c.post(
            &format!("/verification/{id}/sync"),
            json!({"decompositionId": decomp, "taskId": task, "satisfied": !unsatisfied}),
            true,
        )?),
        VerifyCmd::Report { id } => pretty(&c.get(&format!("/verification/{id}/report"), true)?),
    };
    Ok(())
}
