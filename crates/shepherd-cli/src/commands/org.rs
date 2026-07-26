use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum OrgCmd {
    /// Create an organization.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// List organizations, paged.
    List {
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// Get an organization.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Update an organization.
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// Delete an organization.
    Delete {
        #[arg(long)]
        id: String,
    },
}

pub fn run(cmd: OrgCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        OrgCmd::Create { name, disable } => {
            pretty(&c.post("/organization", json!({"name": name, "enable": !disable}), true)?)
        }
        OrgCmd::List { current, page_size } => {
            pretty(&c.get(&format!("/organization?current={current}&pageSize={page_size}"), true)?)
        }
        OrgCmd::Get { id } => pretty(&c.get(&format!("/organization/{id}"), true)?),
        OrgCmd::Update { id, name, disable } => pretty(&c.put(
            &format!("/organization/{id}"),
            json!({"name": name, "enable": !disable}),
            true,
        )?),
        OrgCmd::Delete { id } => pretty(&c.delete(&format!("/organization/{id}"), true)?),
    };
    Ok(())
}
