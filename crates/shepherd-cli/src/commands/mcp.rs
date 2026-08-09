use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum McpCmd {
    /// Send a raw JSON-RPC call to the server's /mcp endpoint (bearer auth).
    Call {
        /// JSON-RPC method, e.g. tools/list or tools/call.
        #[arg(long)]
        method: String,
        /// JSON-RPC params object (raw JSON string; omit for none).
        #[arg(long = "params-json")]
        params_json: Option<String>,
        /// JSON-RPC id (omit for a notification).
        #[arg(long)]
        id: Option<u64>,
    },
    /// Convenience: list the server's MCP tools (issues a tools/list call).
    Tools,
    /// Convenience: call an MCP tool by name.
    ToolsCall {
        #[arg(long)]
        name: String,
        /// Tool arguments (raw JSON object string).
        #[arg(long = "args-json")]
        args_json: Option<String>,
    },
}

pub fn run(cmd: McpCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        McpCmd::Call { method, params_json, id } => {
            let params: Value = match params_json {
                Some(p) => serde_json::from_str(&p)
                    .map_err(|e| format!("--params-json is not valid JSON: {e}"))?,
                None => Value::Null,
            };
            let mut body = json!({ "jsonrpc": "2.0", "method": method });
            if let Some(i) = id {
                body["id"] = json!(i);
            }
            if params != Value::Null {
                body["params"] = params;
            }
            pretty(&c.post("/mcp", body, true)?)
        }
        McpCmd::Tools => pretty(&c.post(
            "/mcp",
            json!({ "jsonrpc": "2.0", "method": "tools/list", "id": 1 }),
            true,
        )?),
        McpCmd::ToolsCall { name, args_json } => {
            let args: Value = match args_json {
                Some(a) => serde_json::from_str(&a)
                    .map_err(|e| format!("--args-json is not valid JSON: {e}"))?,
                None => json!({}),
            };
            pretty(&c.post(
                "/mcp",
                json!({ "jsonrpc": "2.0", "method": "tools/call", "id": 1,
                       "params": { "name": name, "arguments": args } }),
                true,
            )?)
        }
    };
    Ok(())
}
