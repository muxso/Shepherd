use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum AgentCmd {
    /// Connect an AI executor (claude-code | codex | opencode | codebuddy) and save it as the dispatch default.
    Connect {
        #[arg(long = "type")]
        kind: String,
    },
    /// Show current connection / login / server status.
    Status,
    /// Disconnect (dispatch falls back to CLAUDE_CODE).
    Disconnect,
}

pub fn run(cmd: AgentCmd) -> R<()> {
    match cmd {
        AgentCmd::Connect { kind } => {
            let executor = normalize_agent(&kind)?;
            let mut cfg = Config::load();
            let healthy = Client::new(cfg.clone())?.get("/healthz", false).is_ok();
            cfg.agent = Some(executor.clone());
            cfg.save()?;
            println!(
                " connected agent: {executor}  server {} {}",
                cfg.url,
                if healthy { "(reachable)" } else { "(not reachable yet)" }
            );
        }
        AgentCmd::Status => {
            let cfg = Config::load();
            let healthy = Client::new(cfg.clone())?.get("/healthz", false).is_ok();
            println!("server : {}", cfg.url);
            println!("API key: {}", if cfg.api_key.is_empty() { "not set" } else { "set" });
            println!(
                "agent  : {}",
                cfg.agent.as_deref().unwrap_or("(not connected, defaults to CLAUDE_CODE)")
            );
            println!("health : {}", if healthy { "reachable" } else { "unreachable" });
        }
        AgentCmd::Disconnect => {
            let mut cfg = Config::load();
            cfg.agent = None;
            cfg.save()?;
            println!("agent disconnected (dispatch falls back to CLAUDE_CODE)");
        }
    };
    Ok(())
}
