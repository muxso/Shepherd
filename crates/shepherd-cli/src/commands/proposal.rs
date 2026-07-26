use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ProposalCmd {
    /// Create a proposal for a requirement.
    Create {
        #[arg(long)]
        requirement: String,
        #[arg(long)]
        title: String,
    },
    /// List proposals of a requirement.
    List {
        #[arg(long)]
        requirement: String,
    },
    /// Get a proposal.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Submit the design doc for a proposal.
    Design {
        #[arg(long)]
        id: String,
        #[arg(long)]
        doc: String,
    },
    /// Approve a proposal.
    Approve {
        #[arg(long)]
        id: String,
    },
    /// Request changes on a proposal.
    RequestChanges {
        #[arg(long)]
        id: String,
        #[arg(long)]
        comment: String,
    },
}

pub fn run(cmd: ProposalCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        ProposalCmd::Create { requirement, title } => pretty(&c.post(
            "/proposal",
            json!({"requirementId": requirement, "title": title}),
            true,
        )?),
        ProposalCmd::List { requirement } => {
            pretty(&c.get(&format!("/proposal?requirementId={requirement}"), true)?)
        }
        ProposalCmd::Get { id } => pretty(&c.get(&format!("/proposal/{id}"), true)?),
        ProposalCmd::Design { id, doc } => {
            pretty(&c.post(&format!("/proposal/{id}/design"), json!({"doc": doc}), true)?)
        }
        ProposalCmd::Approve { id } => {
            pretty(&c.post(&format!("/proposal/{id}/approve"), json!({}), true)?)
        }
        ProposalCmd::RequestChanges { id, comment } => pretty(&c.post(
            &format!("/proposal/{id}/request-changes"),
            json!({"comment": comment}),
            true,
        )?),
    };
    Ok(())
}
