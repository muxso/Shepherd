use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum UserCmd {
    /// Create a user.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        email: String,
    },
    /// List users, paged.
    List {
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// Get a user.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Update a user.
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        email: String,
        /// Mark disabled (enabled by default).
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// Delete a user.
    Delete {
        #[arg(long)]
        id: String,
    },
    /// Resolve user names by ids in bulk (comma-separated).
    Names {
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,
    },
}

pub fn run(cmd: UserCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        UserCmd::Create { name, email } => {
            pretty(&c.post("/system/user", json!({"name": name, "email": email}), true)?)
        }
        UserCmd::List { current, page_size } => {
            pretty(&c.get(&format!("/system/user?current={current}&pageSize={page_size}"), true)?)
        }
        UserCmd::Get { id } => pretty(&c.get(&format!("/system/user/{id}"), true)?),
        UserCmd::Update { id, name, email, disable } => pretty(&c.put(
            &format!("/system/user/{id}"),
            json!({"name": name, "email": email, "enable": !disable}),
            true,
        )?),
        UserCmd::Delete { id } => pretty(&c.delete(&format!("/system/user/{id}"), true)?),
        UserCmd::Names { ids } => {
            pretty(&c.get(&format!("/system/user/names?ids={}", ids.join(",")), true)?)
        }
    };
    Ok(())
}
