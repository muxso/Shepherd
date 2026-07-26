use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ApikeyCmd {
    /// Create an API key for any user (admin; requires APIKEY:ADD). Prints the raw key once.
    Create {
        #[arg(long)]
        name: String,
        /// Permission strings, comma-separated (e.g. PROJECT:READ+ADD).
        #[arg(long, value_delimiter = ',')]
        permissions: Vec<String>,
    },
    /// Create an API key for yourself. Prints the raw key once.
    CreateMine {
        #[arg(long)]
        name: Option<String>,
        /// Time-to-live in seconds (omit = never expires).
        #[arg(long = "ttl-secs")]
        ttl_secs: Option<i64>,
    },
    /// List all API keys (admin).
    List,
    /// List your own API keys.
    Mine,
    /// Revoke (delete) an API key by id.
    Delete {
        #[arg(long)]
        id: String,
    },
    /// Enable or disable an API key.
    Enable {
        #[arg(long)]
        id: String,
        /// Disable instead of enable.
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
}

pub fn run(cmd: ApikeyCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        ApikeyCmd::Create { name, permissions } => {
            let v =
                c.post("/system/apikey", json!({"name": name, "permissions": permissions}), true)?;
            print_key(&v);
        }
        ApikeyCmd::CreateMine { name, ttl_secs } => {
            let mut body = json!({});
            if let Some(n) = name {
                body["name"] = json!(n);
            }
            if let Some(t) = ttl_secs {
                body["ttlSecs"] = json!(t);
            }
            let v = c.post("/system/apikey/mine", body, true)?;
            print_key(&v);
        }
        ApikeyCmd::List => pretty(&c.get("/system/apikey", true)?),
        ApikeyCmd::Mine => pretty(&c.get("/system/apikey/mine", true)?),
        ApikeyCmd::Delete { id } => {
            c.delete(&format!("/system/apikey/{id}"), true)?;
            println!(" 已吊销 {id}");
        }
        ApikeyCmd::Enable { id, disable } => {
            c.put(&format!("/system/apikey/{id}/enabled"), json!({"enabled": !disable}), true)?;
            println!(" API key {id} 已{}", if disable { "禁用" } else { "启用" });
        }
    };
    Ok(())
}
