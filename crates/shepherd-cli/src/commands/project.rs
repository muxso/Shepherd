use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ProjectCmd {
    /// Create a project.
    Create {
        #[arg(long)]
        org: String,
        #[arg(long)]
        name: String,
    },
    /// List projects in an organization, paged.
    List {
        #[arg(long)]
        org: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
}

pub fn run(cmd: ProjectCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        ProjectCmd::Create { org, name } => {
            pretty(&c.post("/project", json!({"organizationId": org, "name": name}), true)?)
        }
        ProjectCmd::List { org, current, page_size } => pretty(&c.get(
            &format!("/project?organizationId={org}&current={current}&pageSize={page_size}"),
            true,
        )?),
    };
    Ok(())
}
