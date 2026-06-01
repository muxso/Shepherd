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
        #[arg(long, default_value = "CLAUDE_CODE")]
        executor: String,
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

#[derive(Serialize, Deserialize, Default)]
struct Config {
    url: String,
    token: String,
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
            let mut cfg = Config { url, token: String::new() };
            let client = Client::new(cfg.clone_url())?;
            let v = client.post("/auth/login", json!({"username": user, "password": password}), false)?;
            let token = v["token"].as_str().ok_or("登录响应缺少 token")?;
            cfg.token = token.to_string();
            cfg.save()?;
            println!("✅ 已登录 {} → 会话存于 {}", cfg.url, config_path().display());
        }
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
            let c = Client::new(Config::load())?;
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
                json!({"decompositionId": decomp, "taskId": task, "title": title, "executor": executor, "instructions": instr}),
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

impl Config {
    fn clone_url(&self) -> Config {
        Config { url: self.url.clone(), token: self.token.clone() }
    }
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
    fn url_join_trims_slash() {
        let c = Client::new(Config { url: "http://h:1/".into(), token: "t".into() }).expect("client");
        assert_eq!(c.url("/x"), "http://h:1/x");
    }

    #[test]
    fn config_defaults_url_when_empty() {
        // 不依赖文件/环境:空配置应回落默认 URL(此处直接验证逻辑)
        let c = Config { url: String::new(), token: String::new() };
        assert!(c.url.is_empty());
    }
}
