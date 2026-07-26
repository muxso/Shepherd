use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum DebugCmd {
    /// Send a request through the server's debug proxy (supports http + probe protocols).
    Send {
        #[arg(long, default_value = "http")]
        protocol: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long)]
        url: String,
        /// Extra headers, repeatable, "Name: value".
        #[arg(long = "header")]
        headers: Vec<String>,
        /// Request body (string).
        #[arg(long)]
        body: Option<String>,
        /// Protocol-specific params, repeatable, key=value (e.g. method=POST).
        #[arg(long = "meta")]
        meta: Vec<String>,
        /// Assertion array (raw JSON), e.g. '[{"type":"StatusIs","args":200}]'.
        #[arg(long = "assertions-json")]
        assertions_json: Option<String>,
    },
    /// List protocols supported by the debug proxy.
    Protocols,
}

pub fn run(cmd: DebugCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        DebugCmd::Send { protocol, method, url, headers, body, meta, assertions_json } => {
            let mut hdr_arr = Vec::with_capacity(headers.len());
            for h in &headers {
                let (k, v) = h
                    .split_once(':')
                    .ok_or_else(|| format!("--header 需 'Name: value' 格式:{h}"))?;
                hdr_arr.push(json!({ "key": k.trim(), "value": v.trim() }));
            }
            let mut metadata = serde_json::Map::new();
            for kv in &meta {
                if let Some((k, v)) = kv.split_once('=') {
                    metadata.insert(k.trim().to_string(), json!(v.trim()));
                }
            }
            let assertions: Value = match assertions_json {
                Some(aj) => serde_json::from_str(&aj)
                    .map_err(|e| format!("--assertions-json 不是合法 JSON: {e}"))?,
                None => json!([]),
            };
            let req = json!({
                "protocol": if protocol.is_empty() { Value::Null } else { json!(protocol) },
                "method": method.to_uppercase(),
                "url": url,
                "headers": hdr_arr,
                "body": body,
                "meta": metadata,
                "assertions": assertions,
            });
            pretty(&c.post("/api/debug/send", req, true)?)
        }
        DebugCmd::Protocols => pretty(&c.get("/api/debug/protocols", true)?),
    };
    Ok(())
}
