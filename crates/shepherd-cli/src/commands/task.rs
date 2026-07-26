use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum TaskCmd {
    /// Add a task to a decomposition graph.
    Add {
        #[arg(long)]
        decomp: String,
        #[arg(long)]
        title: String,
        /// Dependency task local ids, comma-separated.
        #[arg(long, value_delimiter = ',')]
        deps: Vec<String>,
    },
}

pub fn run(cmd: TaskCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        TaskCmd::Add { decomp, title, deps } => pretty(&c.post(
            &format!("/decomposition/{decomp}/task"),
            json!({"title": title, "dependencies": deps}),
            true,
        )?),
    };
    Ok(())
}
