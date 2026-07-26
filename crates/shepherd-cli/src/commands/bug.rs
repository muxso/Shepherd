use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum BugCmd {
    /// Create a bug.
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "NEW")]
        status: String,
    },
    /// Transition bug status.
    Status {
        #[arg(long)]
        id: String,
        #[arg(long)]
        to: String,
    },
}

pub fn run(cmd: BugCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        BugCmd::Create { project, title, status } => pretty(&c.post(
            "/bug",
            json!({"projectId": project, "title": title, "initialStatus": status}),
            true,
        )?),
        BugCmd::Status { id, to } => {
            pretty(&c.post(&format!("/bug/{id}/status"), json!({"status": to}), true)?)
        }
    };
    Ok(())
}
