use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum CaseCmd {
    /// Submit a case review comment.
    Review {
        #[arg(long)]
        review: String,
        #[arg(long)]
        case: String,
        #[arg(long)]
        reviewer: String,
        /// PASS | UN_PASS | UNDER_REVIEWED.
        #[arg(long)]
        status: String,
        /// Required for UN_PASS.
        #[arg(long)]
        content: Option<String>,
    },
}

pub fn run(cmd: CaseCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        CaseCmd::Review { review, case, reviewer, status, content } => pretty(&c.post(
            &format!("/case-review/{review}/{case}"),
            json!({"reviewerId": reviewer, "status": status, "content": content}),
            true,
        )?),
    };
    Ok(())
}
