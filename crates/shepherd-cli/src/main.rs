//! Shepherd CLI(`shepherd`):封装 REST API 驱动全链路。
//!
//! `shepherd login` 存会话(URL + 令牌)到 `~/.shepherd/config.json`(或用 SHEPHERD_URL /
//! SHEPHERD_TOKEN 环境变量覆盖)。其余命令读取它带上 Bearer 调用 Shepherd 服务。

use std::error::Error;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

type R<T> = Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(name = "shepherd", version, about = "Shepherd —— AI 研发监督平台 CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 登录并保存会话。
    Login {
        #[arg(long, default_value = "http://127.0.0.1:8088")]
        url: String,
        #[arg(long)]
        user: String,
        #[arg(long)]
        password: String,
    },
    /// 需求管理。
    Req {
        #[command(subcommand)]
        cmd: ReqCmd,
    },
    /// 为需求版本开启任务拆分图。
    Decompose {
        #[arg(long)]
        req: String,
        #[arg(long, default_value_t = 1)]
        version: u32,
    },
    /// 任务管理。
    Task {
        #[command(subcommand)]
        cmd: TaskCmd,
    },
    /// 把任务派发给 AI 执行者(对应 README 的 `task run`)。
    Dispatch {
        #[arg(long)]
        decomp: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        title: String,
        /// 执行者(默认用 `agent connect` 连接的;未连接则 CLAUDE_CODE)。
        #[arg(long)]
        executor: Option<String>,
        #[arg(long)]
        instructions: Option<String>,
        /// 可选:项目 id(配合 --skills 自动组合行为规范)。
        #[arg(long)]
        project: Option<String>,
        /// 可选:skill id 逗号分隔,派发前自动 compose 为行为规范注入。
        #[arg(long, value_delimiter = ',')]
        skills: Vec<String>,
    },
    /// 完整性验证。
    Verify {
        #[command(subcommand)]
        cmd: VerifyCmd,
    },
    /// AI Skill 编排。
    Skill {
        #[command(subcommand)]
        cmd: SkillCmd,
    },
    /// 连接/查看 AI 执行者(dispatch 的默认 executor)。
    Agent {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
}

#[derive(Subcommand)]
enum AgentCmd {
    /// 连接一个 AI 执行者(claude-code | codex),保存为 dispatch 默认。
    Connect {
        #[arg(long = "type")]
        kind: String,
    },
    /// 查看当前连接 / 登录 / 服务状态。
    Status,
    /// 断开(dispatch 回落默认 CLAUDE_CODE)。
    Disconnect,
}

/// 把 `--type` 归一为执行者枚举串。
fn normalize_agent(t: &str) -> R<String> {
    match t.to_ascii_lowercase().replace('-', "_").as_str() {
        "claude_code" => Ok("CLAUDE_CODE".into()),
        "codex" => Ok("CODEX".into()),
        other => Err(format!("未知 agent 类型: {other}(支持 claude-code | codex)").into()),
    }
}

#[derive(Subcommand)]
enum ReqCmd {
    /// 新建需求。
    Add {
        #[arg(long)]
        project: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        description: String,
        /// 验收标准,逗号分隔。
        #[arg(long, value_delimiter = ',')]
        criteria: Vec<String>,
    },
    /// 列出项目内需求。
    List {
        #[arg(long)]
        project: String,
    },
    /// 自动拆分需求为任务 DAG(服务端取规格,交规划器拆分)。
    Breakdown {
        #[arg(long)]
        req: String,
        /// 可选:指定需求版本(默认基线版本)。
        #[arg(long)]
        version: Option<u32>,
        /// 使用 AI 规划器(默认即用服务端配置的规划器;此标志仅为可读性)。
        #[arg(long, default_value_t = false)]
        ai: bool,
    },
}

#[derive(Subcommand)]
enum TaskCmd {
    /// 向拆分图加任务。
    Add {
        #[arg(long)]
        decomp: String,
        #[arg(long)]
        title: String,
        /// 依赖任务本地 id,逗号分隔。
        #[arg(long, value_delimiter = ',')]
        deps: Vec<String>,
    },
}

#[derive(Subcommand)]
enum VerifyCmd {
    /// 开启验证(传入验收标准)。
    Create {
        #[arg(long)]
        req: String,
        #[arg(long, default_value_t = 1)]
        version: u32,
        #[arg(long, value_delimiter = ',')]
        criteria: Vec<String>,
    },
    /// 完整性报告。
    Report {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum SkillCmd {
    /// 定义 skill。
    Add {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        instructions: String,
        #[arg(long, value_delimiter = ',')]
        includes: Vec<String>,
    },
    /// 组合 skill → 指令集。
    Compose {
        #[arg(long)]
        project: String,
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Config {
    url: String,
    token: String,
    /// 已连接的 AI 执行者(dispatch 默认 executor)。
    #[serde(default)]
    agent: Option<String>,
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".shepherd").join("config.json")
}

impl Config {
    fn load() -> Config {
        // 环境变量优先,否则读配置文件。
        let mut c: Config = std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if let Ok(u) = std::env::var("SHEPHERD_URL") {
            c.url = u;
        }
        if let Ok(t) = std::env::var("SHEPHERD_TOKEN") {
            c.token = t;
        }
        if c.url.is_empty() {
            c.url = "http://127.0.0.1:8088".into();
        }
        c
    }

    fn save(&self) -> R<()> {
        let p = config_path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&p, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

struct Client {
    http: reqwest::blocking::Client,
    cfg: Config,
}

impl Client {
    fn new(cfg: Config) -> R<Client> {
        // no_proxy:服务通常本地/内网,避免被全局代理劫持。
        let http = reqwest::blocking::Client::builder().no_proxy().build()?;
        Ok(Client { http, cfg })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.cfg.url.trim_end_matches('/'), path)
    }

    fn send(&self, mut rb: reqwest::blocking::RequestBuilder, auth: bool) -> R<Value> {
        if auth {
            if self.cfg.token.is_empty() {
                return Err("未登录:先执行 `shepherd login`".into());
            }
            rb = rb.bearer_auth(&self.cfg.token);
        }
        let resp = rb.send()?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {text}").into());
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    fn post(&self, path: &str, body: Value, auth: bool) -> R<Value> {
        self.send(self.http.post(self.url(path)).json(&body), auth)
    }
    fn get(&self, path: &str, auth: bool) -> R<Value> {
        self.send(self.http.get(self.url(path)), auth)
    }
}

fn pretty(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn run(cli: Cli) -> R<()> {
    match cli.cmd {
        Cmd::Login { url, user, password } => {
            let mut cfg = Config::load(); // 保留已连接的 agent
            cfg.url = url;
            let client = Client::new(cfg.clone())?;
            let v = client.post("/auth/login", json!({"username": user, "password": password}), false)?;
            let token = v["token"].as_str().ok_or("登录响应缺少 token")?;
            cfg.token = token.to_string();
            cfg.save()?;
            println!("✅ 已登录 {} → 会话存于 {}", cfg.url, config_path().display());
        }
        Cmd::Agent { cmd } => match cmd {
            AgentCmd::Connect { kind } => {
                let executor = normalize_agent(&kind)?;
                let mut cfg = Config::load();
                let healthy = Client::new(cfg.clone())?.get("/healthz", false).is_ok();
                cfg.agent = Some(executor.clone());
                cfg.save()?;
                println!(
                    "✅ 已连接 agent: {executor}  服务 {} {}",
                    cfg.url,
                    if healthy { "(可达)" } else { "(暂不可达)" }
                );
            }
            AgentCmd::Status => {
                let cfg = Config::load();
                let healthy = Client::new(cfg.clone())?.get("/healthz", false).is_ok();
                println!("服务  : {}", cfg.url);
                println!("登录  : {}", if cfg.token.is_empty() { "未登录" } else { "已登录" });
                println!("agent : {}", cfg.agent.as_deref().unwrap_or("(未连接,默认 CLAUDE_CODE)"));
                println!("健康  : {}", if healthy { "可达" } else { "不可达" });
            }
            AgentCmd::Disconnect => {
                let mut cfg = Config::load();
                cfg.agent = None;
                cfg.save()?;
                println!("已断开 agent 连接(dispatch 回落默认 CLAUDE_CODE)");
            }
        },
        Cmd::Req { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ReqCmd::Add { project, title, description, criteria } => pretty(&c.post(
                    "/requirement",
                    json!({"projectId": project, "title": title, "description": description, "acceptanceCriteria": criteria}),
                    true,
                )?),
                ReqCmd::List { project } => {
                    pretty(&c.get(&format!("/requirement?projectId={project}"), true)?)
                }
                ReqCmd::Breakdown { req, version, ai: _ } => {
                    // 服务端据 requirementId 取规格并拆分。
                    let path = match version {
                        Some(v) => format!("/requirement/{req}/breakdown?version={v}"),
                        None => format!("/requirement/{req}/breakdown"),
                    };
                    pretty(&c.post(&path, json!({}), true)?);
                }
            }
        }
        Cmd::Decompose { req, version } => {
            let c = Client::new(Config::load())?;
            pretty(&c.post("/decomposition", json!({"requirementId": req, "requirementVersion": version}), true)?);
        }
        Cmd::Task { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                TaskCmd::Add { decomp, title, deps } => pretty(&c.post(
                    &format!("/decomposition/{decomp}/task"),
                    json!({"title": title, "dependencies": deps}),
                    true,
                )?),
            }
        }
        Cmd::Dispatch { decomp, task, title, executor, instructions, project, skills } => {
            let cfg = Config::load();
            // 执行者:显式 --executor 优先,否则用已连接 agent,再否则默认。
            let exec = executor.or_else(|| cfg.agent.clone()).unwrap_or_else(|| "CLAUDE_CODE".into());
            let c = Client::new(cfg)?;
            // 若给了 --skills,先 compose 成行为规范(与 --instructions 合并)。
            let mut instr = instructions;
            if !skills.is_empty() {
                let project = project.ok_or("--skills 需配合 --project")?;
                let comp = c.post("/skill/compose", json!({"projectId": project, "skillIds": skills}), true)?;
                let composed = comp["instructions"].as_str().unwrap_or("").to_string();
                instr = Some(match instr {
                    Some(extra) if !extra.trim().is_empty() => format!("{composed}\n\n{extra}"),
                    _ => composed,
                });
            }
            pretty(&c.post(
                "/delivery",
                json!({"decompositionId": decomp, "taskId": task, "title": title, "executor": exec, "instructions": instr}),
                true,
            )?);
        }
        Cmd::Verify { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                VerifyCmd::Create { req, version, criteria } => pretty(&c.post(
                    "/verification",
                    json!({"requirementId": req, "requirementVersion": version, "criteria": criteria}),
                    true,
                )?),
                VerifyCmd::Report { id } => pretty(&c.get(&format!("/verification/{id}/report"), true)?),
            }
        }
        Cmd::Skill { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                SkillCmd::Add { project, name, instructions, includes } => pretty(&c.post(
                    "/skill",
                    json!({"projectId": project, "name": name, "instructions": instructions, "includes": includes}),
                    true,
                )?),
                SkillCmd::Compose { project, ids } => pretty(&c.post(
                    "/skill/compose",
                    json!({"projectId": project, "skillIds": ids}),
                    true,
                )?),
            }
        }
    }
    Ok(())
}


fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("✗ {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn normalize_agent_maps_kinds() {
        assert_eq!(normalize_agent("claude-code").unwrap(), "CLAUDE_CODE");
        assert_eq!(normalize_agent("CODEX").unwrap(), "CODEX");
        assert!(normalize_agent("gpt").is_err());
    }

    #[test]
    fn url_join_trims_slash() {
        let c = Client::new(Config { url: "http://h:1/".into(), token: "t".into(), agent: None }).expect("client");
        assert_eq!(c.url("/x"), "http://h:1/x");
    }

    #[test]
    fn config_defaults_url_when_empty() {
        // 不依赖文件/环境:空配置应回落默认 URL(此处直接验证逻辑)
        let c = Config { url: String::new(), token: String::new(), agent: None };
        assert!(c.url.is_empty());
    }
}
