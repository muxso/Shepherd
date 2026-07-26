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
                " 已连接 agent: {executor}  服务 {} {}",
                cfg.url,
                if healthy { "(可达)" } else { "(暂不可达)" }
            );
        }
        AgentCmd::Status => {
            let cfg = Config::load();
            let healthy = Client::new(cfg.clone())?.get("/healthz", false).is_ok();
            println!("服务  : {}", cfg.url);
            println!("API key: {}", if cfg.api_key.is_empty() { "未配置" } else { "已配置" });
            println!("agent : {}", cfg.agent.as_deref().unwrap_or("(未连接,默认 CLAUDE_CODE)"));
            println!("健康  : {}", if healthy { "可达" } else { "不可达" });
        }
        AgentCmd::Disconnect => {
            let mut cfg = Config::load();
            cfg.agent = None;
            cfg.save()?;
            println!("已断开 agent 连接(dispatch 回落默认 CLAUDE_CODE)");
        }
    };
    Ok(())
}
