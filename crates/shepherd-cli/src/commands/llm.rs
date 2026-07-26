use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum LlmCmd {
    /// List your configured LLM models.
    List,
    /// Add an LLM model.
    Create {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long)]
        api_key: Option<String>,
    },
    /// Update an LLM model (only the provided fields change).
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long)]
        api_key: Option<String>,
        /// Set enabled.
        #[arg(long, default_value_t = false)]
        enable: bool,
        /// Set disabled.
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// Delete an LLM model.
    Delete {
        #[arg(long)]
        id: String,
    },
}

pub fn run(cmd: LlmCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        LlmCmd::List => pretty(&c.get("/me/llm-model", true)?),
        LlmCmd::Create { provider, name, base_url, api_key } => {
            let mut body = json!({"provider": provider, "name": name});
            if let Some(b) = base_url {
                body["baseUrl"] = json!(b);
            }
            if let Some(k) = api_key {
                body["apiKey"] = json!(k);
            }
            pretty(&c.post("/me/llm-model", body, true)?);
        }
        LlmCmd::Update { id, name, base_url, api_key, enable, disable } => {
            let mut patch = json!({});
            if let Some(n) = name {
                patch["name"] = json!(n);
            }
            if let Some(b) = base_url {
                patch["baseUrl"] = json!(b);
            }
            if let Some(k) = api_key {
                patch["apiKey"] = json!(k);
            }
            if disable {
                patch["enabled"] = json!(false);
            } else if enable {
                patch["enabled"] = json!(true);
            }
            pretty(&c.put(&format!("/me/llm-model/{id}"), patch, true)?);
        }
        LlmCmd::Delete { id } => {
            c.delete(&format!("/me/llm-model/{id}"), true)?;
            println!(" 已删除 LLM model {id}");
        }
    };
    Ok(())
}
