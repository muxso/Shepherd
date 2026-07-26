use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum PrdCmd {
    /// Draft a requirement (title/description/acceptance criteria) from raw material.
    Draft {
        #[arg(long)]
        raw: String,
    },
}

pub fn run(cmd: PrdCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        PrdCmd::Draft { raw } => {
            pretty(&c.post("/requirement/draft", json!({"raw": raw}), true)?)
        }
    };
    Ok(())
}
