use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum RoleCmd {
    /// Create a role.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        scope: Option<String>,
        /// Permission strings, comma-separated (e.g. PROJECT:READ+ADD).
        #[arg(long, value_delimiter = ',')]
        permissions: Vec<String>,
    },
    /// List roles, paged.
    List {
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// Get a role.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Update a role.
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        scope: Option<String>,
        /// Permission strings, comma-separated (e.g. PROJECT:READ+ADD).
        #[arg(long, value_delimiter = ',')]
        permissions: Vec<String>,
    },
    /// Delete a role.
    Delete {
        #[arg(long)]
        id: String,
    },
    /// Grant a role to a user.
    Grant {
        #[arg(long)]
        user: String,
        #[arg(long)]
        role: String,
    },
    /// Revoke a user's role.
    Revoke {
        #[arg(long)]
        user: String,
        #[arg(long)]
        role: String,
    },
}

pub fn run(cmd: RoleCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        RoleCmd::Create { name, scope, permissions } => pretty(&c.post(
            "/role",
            json!({"name": name, "scope": scope, "permissions": permissions}),
            true,
        )?),
        RoleCmd::List { current, page_size } => {
            pretty(&c.get(&format!("/role?current={current}&pageSize={page_size}"), true)?)
        }
        RoleCmd::Get { id } => pretty(&c.get(&format!("/role/{id}"), true)?),
        RoleCmd::Update { id, name, scope, permissions } => pretty(&c.put(
            &format!("/role/{id}"),
            json!({"name": name, "scope": scope, "permissions": permissions}),
            true,
        )?),
        RoleCmd::Delete { id } => pretty(&c.delete(&format!("/role/{id}"), true)?),
        RoleCmd::Grant { user, role } => {
            pretty(&c.post("/user-role/grant", json!({"userId": user, "roleId": role}), true)?)
        }
        RoleCmd::Revoke { user, role } => {
            pretty(&c.post("/user-role/revoke", json!({"userId": user, "roleId": role}), true)?)
        }
    };
    Ok(())
}
