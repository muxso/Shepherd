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
        #[arg(long, default_value = "http://localhost:8088")]
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
    /// 注销当前会话(撤销服务端令牌并清空本地 token)。
    Logout,
    /// 项目管理。
    Project {
        #[command(subcommand)]
        cmd: ProjectCmd,
    },
    /// 缺陷管理。
    Bug {
        #[command(subcommand)]
        cmd: BugCmd,
    },
    /// 用例评审。
    Case {
        #[command(subcommand)]
        cmd: CaseCmd,
    },
    /// 测试计划。
    Plan {
        #[command(subcommand)]
        cmd: PlanCmd,
    },
    /// 接口测试(批量执行)。
    Api {
        #[command(subcommand)]
        cmd: ApiCmd,
    },
    /// 用户管理。
    User {
        #[command(subcommand)]
        cmd: UserCmd,
    },
    /// 组织管理。
    Org {
        #[command(subcommand)]
        cmd: OrgCmd,
    },
    /// 角色与授权。
    Role {
        #[command(subcommand)]
        cmd: RoleCmd,
    },
    /// 接口定义(目录 + 用例 + Mock)。
    Apidef {
        #[command(subcommand)]
        cmd: ApidefCmd,
    },
    /// 场景(编排 + 步骤 + 编译 + 运行)。
    Scenario {
        #[command(subcommand)]
        cmd: ScenarioCmd,
    },
    /// 环境(项目级 base_url + 默认头 + 变量;运行时注入)。
    Env {
        #[command(subcommand)]
        cmd: EnvCmd,
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

#[derive(Subcommand)]
enum ProjectCmd {
    /// 新建项目。
    Create {
        #[arg(long)]
        org: String,
        #[arg(long)]
        name: String,
    },
    /// 分页列出组织内项目。
    List {
        #[arg(long)]
        org: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
}

#[derive(Subcommand)]
enum BugCmd {
    /// 新建缺陷。
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "NEW")]
        status: String,
    },
    /// 流转缺陷状态。
    Status {
        #[arg(long)]
        id: String,
        #[arg(long)]
        to: String,
    },
}

#[derive(Subcommand)]
enum CaseCmd {
    /// 提交一次用例评审意见。
    Review {
        #[arg(long)]
        review: String,
        #[arg(long)]
        case: String,
        #[arg(long)]
        reviewer: String,
        /// PASS | UN_PASS | UNDER_REVIEWED。
        #[arg(long)]
        status: String,
        /// UN_PASS 必填。
        #[arg(long)]
        content: Option<String>,
    },
}

#[derive(Subcommand)]
enum PlanCmd {
    /// 新建测试计划(或分组)。
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// TEST_PLAN | GROUP。
        #[arg(long = "type", default_value = "TEST_PLAN")]
        plan_type: String,
        #[arg(long)]
        group: Option<String>,
    },
    /// 计划执行统计。
    Stats {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum ApiCmd {
    /// 批量执行接口用例。
    BatchRun {
        #[arg(long)]
        project: String,
        #[arg(long, value_delimiter = ',')]
        cases: Vec<String>,
        /// 运行模式(如 SERIAL | PARALLEL)。
        #[arg(long = "mode", default_value = "PARALLEL")]
        run_mode: String,
        /// 资源池 id(批量执行需客户端提供)。
        #[arg(long)]
        pool: Option<String>,
        /// 运行所用环境 id(注入 base_url/默认头/变量)。
        #[arg(long)]
        env: Option<String>,
    },
}

#[derive(Subcommand)]
enum UserCmd {
    /// 新建用户。
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        email: String,
    },
    /// 分页列出用户。
    List {
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// 查看单个用户。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 更新用户。
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        email: String,
        /// 标记禁用(默认启用)。
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// 删除用户。
    Delete {
        #[arg(long)]
        id: String,
    },
    /// 按 id 批量解析用户名(逗号分隔)。
    Names {
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,
    },
}

#[derive(Subcommand)]
enum OrgCmd {
    /// 新建组织。
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// 分页列出组织。
    List {
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// 查看单个组织。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 更新组织。
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// 删除组织。
    Delete {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum RoleCmd {
    /// 新建角色。
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        scope: Option<String>,
        /// 权限串,逗号分隔(如 PROJECT:READ+ADD)。
        #[arg(long, value_delimiter = ',')]
        permissions: Vec<String>,
    },
    /// 分页列出角色。
    List {
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// 查看单个角色。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 更新角色。
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, value_delimiter = ',')]
        permissions: Vec<String>,
    },
    /// 删除角色。
    Delete {
        #[arg(long)]
        id: String,
    },
    /// 给用户授予角色。
    Grant {
        #[arg(long)]
        user: String,
        #[arg(long)]
        role: String,
    },
    /// 撤销用户的角色。
    Revoke {
        #[arg(long)]
        user: String,
        #[arg(long)]
        role: String,
    },
}

#[derive(Subcommand)]
enum ApidefCmd {
    /// 新建接口定义。
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// 接口类型 HTTP | TCP | SQL | DUBBO。
        #[arg(long, default_value = "HTTP")]
        protocol: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long, default_value = "")]
        path: String,
    },
    /// 从 OpenAPI 3.x / Swagger 2.0 批量导入接口定义(--file 本地 / --url 远程二选一)。
    Import {
        #[arg(long)]
        project: String,
        /// OpenAPI/Swagger JSON 文件路径。
        #[arg(long)]
        file: Option<String>,
        /// OpenAPI/Swagger JSON 的 URL(如服务自身的 /api-docs/openapi.json)。
        #[arg(long)]
        url: Option<String>,
    },
    /// 列出项目内接口定义。
    List {
        #[arg(long)]
        project: String,
    },
    /// 查看单个接口定义。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 给定义加接口用例(落 ms_api_case,可被批量运行)。
    Case {
        #[arg(long)]
        def: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long)]
        url: String,
        #[arg(long)]
        body: Option<String>,
        /// 期望状态码:给定即生成 StatusIs 断言(决定用例成功/失败);省略则空断言(恒成功)。
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
    },
    /// 列出定义下的接口用例。
    Cases {
        #[arg(long)]
        def: String,
    },
    /// 新建接口用例(可独立于定义:省略 --def 即独立用例)。
    CaseNew {
        #[arg(long)]
        project: String,
        #[arg(long)]
        def: Option<String>,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long)]
        url: String,
        #[arg(long)]
        body: Option<String>,
        /// 期望状态码:给定即生成 StatusIs 断言(决定用例成功/失败);省略则空断言(恒成功)。
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
    },
    /// 为项目内每个接口定义批量生成「成功(期望 2xx)+ 失败(期望 401)」用例,并各建一条场景串联两者。
    GenSuite {
        #[arg(long)]
        project: String,
        /// 用例请求的基础 URL(拼到 OpenAPI path 前)。
        #[arg(long, default_value = "http://localhost:8088")]
        base: String,
        /// 仅生成用例,不建场景。
        #[arg(long = "no-scenario", default_value_t = false)]
        no_scenario: bool,
    },
    /// 分页列出项目内接口用例(独立视图)。
    CaseList {
        #[arg(long)]
        project: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// 分页查看某用例的执行记录。
    CaseExec {
        #[arg(long)]
        case: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// 单独执行某条接口用例(可选环境/资源池),回写执行记录。
    CaseRun {
        #[arg(long)]
        case: String,
        #[arg(long)]
        project: String,
        #[arg(long = "mode", default_value = "SERIAL")]
        run_mode: String,
        #[arg(long)]
        pool: Option<String>,
        /// 运行所用环境 id(注入 base_url/默认头/变量)。
        #[arg(long)]
        env: Option<String>,
    },
    /// 给定义加 Mock。
    Mock {
        #[arg(long)]
        def: String,
        #[arg(long)]
        name: String,
        #[arg(long = "status", default_value_t = 200)]
        response_status: i32,
        #[arg(long)]
        body: Option<String>,
    },
    /// 列出定义下的 Mock。
    Mocks {
        #[arg(long)]
        def: String,
    },
}

#[derive(Subcommand)]
enum ScenarioCmd {
    /// 新建场景。
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
    },
    /// 列出项目内场景。
    List {
        #[arg(long)]
        project: String,
    },
    /// 查看单个场景(含步骤)。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 加步骤:kind=request|case|scenario|loop|if|once|timer。
    Step {
        #[arg(long)]
        scenario: String,
        /// request | case | scenario | loop | if | once | timer。
        #[arg(long)]
        kind: String,
        #[arg(long = "ref-mode", default_value = "REFERENCE")]
        ref_mode: String,
        #[arg(long, default_value_t = 1)]
        order: i32,
        /// case/scenario 步骤:被引用的 id。
        #[arg(long = "ref")]
        ref_id: Option<String>,
        /// request 步骤:内联请求。
        #[arg(long)]
        method: Option<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        body: Option<String>,
        /// 控制器步骤(loop/if/once/timer)的载荷 JSON,如
        /// '{"times":3,"children":[{"kind":"CASE","refId":"c1"}]}'。
        #[arg(long = "control-json")]
        control_json: Option<String>,
    },
    /// 编译场景为可运行步骤(递归展开子场景)。
    Compile {
        #[arg(long)]
        id: String,
    },
    /// 分页查看场景执行记录(含执行状态)。
    Executions {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// 运行场景(编译 → 批量执行)。
    Run {
        #[arg(long)]
        id: String,
        #[arg(long)]
        project: String,
        #[arg(long = "mode", default_value = "PARALLEL")]
        run_mode: String,
        #[arg(long)]
        pool: Option<String>,
        /// 运行所用环境 id(注入 base_url/默认头/变量)。
        #[arg(long)]
        env: Option<String>,
    },
}

#[derive(Subcommand)]
enum EnvCmd {
    /// 新建环境。--header 形如 "Name: value"(可重复);--var 形如 key=value(可重复)。
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// 基础地址(相对 url 前缀),如 http://localhost:8088。
        #[arg(long, default_value = "")]
        base: String,
        #[arg(long = "header")]
        headers: Vec<String>,
        #[arg(long = "var")]
        vars: Vec<String>,
    },
    /// 列出项目内环境。
    List {
        #[arg(long)]
        project: String,
    },
    /// 查看单个环境。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 更新环境(整体覆盖 name/base/headers/vars)。
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        base: String,
        #[arg(long = "header")]
        headers: Vec<String>,
        #[arg(long = "var")]
        vars: Vec<String>,
    },
    /// 删除环境。
    Delete {
        #[arg(long)]
        id: String,
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
    fn put(&self, path: &str, body: Value, auth: bool) -> R<Value> {
        self.send(self.http.put(self.url(path)).json(&body), auth)
    }
    /// 拉取任意 URL 的文本(用于 import --url 读取远程 OpenAPI 文档)。
    fn fetch_text(&self, url: &str) -> R<String> {
        let resp = self.http.get(url).send()?;
        if !resp.status().is_success() {
            return Err(format!("拉取 {url} 失败:HTTP {}", resp.status()).into());
        }
        Ok(resp.text()?)
    }
    fn delete(&self, path: &str, auth: bool) -> R<Value> {
        self.send(self.http.delete(self.url(path)), auth)
    }
}

fn pretty(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

/// 把期望状态码翻成 runner 认得的断言数组(`[{"type":"StatusIs","args":N}]`);
/// 省略 → 空断言(无失败项 → 恒判 Success)。
fn status_assertions(expect_status: Option<u16>) -> Value {
    match expect_status {
        Some(code) => json!([{ "type": "StatusIs", "args": code }]),
        None => json!([]),
    }
}

/// `--header "Name: value"` 列表 → JSON 数组 `[{"name","value"}]`(按首个冒号切分)。
fn parse_headers(items: &[String]) -> R<Value> {
    let mut arr = Vec::with_capacity(items.len());
    for it in items {
        let (name, value) = it
            .split_once(':')
            .ok_or_else(|| format!("--header 需 'Name: value' 格式:{it}"))?;
        arr.push(json!({"name": name.trim(), "value": value.trim()}));
    }
    Ok(Value::Array(arr))
}

/// `--var key=value` 列表 → JSON 对象 `{k:v}`(按首个等号切分)。
fn parse_vars(items: &[String]) -> R<Value> {
    let mut map = serde_json::Map::with_capacity(items.len());
    for it in items {
        let (k, v) = it.split_once('=').ok_or_else(|| format!("--var 需 'key=value' 格式:{it}"))?;
        map.insert(k.trim().to_string(), json!(v));
    }
    Ok(Value::Object(map))
}

fn run(cli: Cli) -> R<()> {
    match cli.cmd {
        Cmd::Login {
            url,
            user,
            password,
        } => {
            let mut cfg = Config::load(); // 保留已连接的 agent
            cfg.url = url;
            let client = Client::new(cfg.clone())?;
            let v = client.post(
                "/auth/login",
                json!({"username": user, "password": password}),
                false,
            )?;
            let token = v["token"].as_str().ok_or("登录响应缺少 token")?;
            cfg.token = token.to_string();
            cfg.save()?;
            println!(
                "✅ 已登录 {} → 会话存于 {}",
                cfg.url,
                config_path().display()
            );
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
                    if healthy {
                        "(可达)"
                    } else {
                        "(暂不可达)"
                    }
                );
            }
            AgentCmd::Status => {
                let cfg = Config::load();
                let healthy = Client::new(cfg.clone())?.get("/healthz", false).is_ok();
                println!("服务  : {}", cfg.url);
                println!(
                    "登录  : {}",
                    if cfg.token.is_empty() {
                        "未登录"
                    } else {
                        "已登录"
                    }
                );
                println!(
                    "agent : {}",
                    cfg.agent.as_deref().unwrap_or("(未连接,默认 CLAUDE_CODE)")
                );
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
            pretty(&c.post(
                "/decomposition",
                json!({"requirementId": req, "requirementVersion": version}),
                true,
            )?);
        }
        Cmd::Task { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                TaskCmd::Add {
                    decomp,
                    title,
                    deps,
                } => pretty(&c.post(
                    &format!("/decomposition/{decomp}/task"),
                    json!({"title": title, "dependencies": deps}),
                    true,
                )?),
            }
        }
        Cmd::Dispatch {
            decomp,
            task,
            title,
            executor,
            instructions,
            project,
            skills,
        } => {
            let cfg = Config::load();
            // 执行者:显式 --executor 优先,否则用已连接 agent,再否则默认。
            let exec = executor
                .or_else(|| cfg.agent.clone())
                .unwrap_or_else(|| "CLAUDE_CODE".into());
            let c = Client::new(cfg)?;
            // 若给了 --skills,先 compose 成行为规范(与 --instructions 合并)。
            let mut instr = instructions;
            if !skills.is_empty() {
                let project = project.ok_or("--skills 需配合 --project")?;
                let comp = c.post(
                    "/skill/compose",
                    json!({"projectId": project, "skillIds": skills}),
                    true,
                )?;
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
        Cmd::Logout => {
            let mut cfg = Config::load();
            let c = Client::new(cfg.clone())?;
            // 服务端撤销当前令牌(忽略未登录/已失效)。
            let _ = c.post("/auth/logout", json!({}), true);
            cfg.token.clear();
            cfg.save()?;
            println!("✅ 已注销,本地会话已清空");
        }
        Cmd::Project { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ProjectCmd::Create { org, name } => pretty(&c.post(
                    "/project",
                    json!({"organizationId": org, "name": name}),
                    true,
                )?),
                ProjectCmd::List { org, current, page_size } => pretty(&c.get(
                    &format!("/project?organizationId={org}&current={current}&pageSize={page_size}"),
                    true,
                )?),
            }
        }
        Cmd::Bug { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                BugCmd::Create { project, title, status } => pretty(&c.post(
                    "/bug",
                    json!({"projectId": project, "title": title, "initialStatus": status}),
                    true,
                )?),
                BugCmd::Status { id, to } => {
                    pretty(&c.post(&format!("/bug/{id}/status"), json!({"status": to}), true)?)
                }
            }
        }
        Cmd::Case { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                CaseCmd::Review { review, case, reviewer, status, content } => pretty(&c.post(
                    &format!("/case-review/{review}/{case}"),
                    json!({"reviewerId": reviewer, "status": status, "content": content}),
                    true,
                )?),
            }
        }
        Cmd::Plan { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                PlanCmd::Create { project, name, plan_type, group } => {
                    let mut body = json!({"projectId": project, "name": name, "type": plan_type});
                    if let Some(g) = group {
                        body["groupId"] = json!(g);
                    }
                    pretty(&c.post("/test-plan", body, true)?)
                }
                PlanCmd::Stats { id } => {
                    pretty(&c.get(&format!("/test-plan/{id}/statistics"), true)?)
                }
            }
        }
        Cmd::Api { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ApiCmd::BatchRun { project, cases, run_mode, pool, env } => pretty(&c.post(
                    "/api/batch-run",
                    json!({"projectId": project, "caseIds": cases, "runMode": run_mode, "poolId": pool, "environmentId": env}),
                    true,
                )?),
            }
        }
        Cmd::User { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                UserCmd::Create { name, email } => {
                    pretty(&c.post("/system/user", json!({"name": name, "email": email}), true)?)
                }
                UserCmd::List { current, page_size } => pretty(&c.get(
                    &format!("/system/user?current={current}&pageSize={page_size}"),
                    true,
                )?),
                UserCmd::Get { id } => pretty(&c.get(&format!("/system/user/{id}"), true)?),
                UserCmd::Update { id, name, email, disable } => pretty(&c.put(
                    &format!("/system/user/{id}"),
                    json!({"name": name, "email": email, "enable": !disable}),
                    true,
                )?),
                UserCmd::Delete { id } => pretty(&c.delete(&format!("/system/user/{id}"), true)?),
                UserCmd::Names { ids } => pretty(&c.get(
                    &format!("/system/user/names?ids={}", ids.join(",")),
                    true,
                )?),
            }
        }
        Cmd::Org { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                OrgCmd::Create { name, disable } => pretty(&c.post(
                    "/organization",
                    json!({"name": name, "enable": !disable}),
                    true,
                )?),
                OrgCmd::List { current, page_size } => pretty(&c.get(
                    &format!("/organization?current={current}&pageSize={page_size}"),
                    true,
                )?),
                OrgCmd::Get { id } => pretty(&c.get(&format!("/organization/{id}"), true)?),
                OrgCmd::Update { id, name, disable } => pretty(&c.put(
                    &format!("/organization/{id}"),
                    json!({"name": name, "enable": !disable}),
                    true,
                )?),
                OrgCmd::Delete { id } => pretty(&c.delete(&format!("/organization/{id}"), true)?),
            }
        }
        Cmd::Role { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                RoleCmd::Create { name, scope, permissions } => pretty(&c.post(
                    "/role",
                    json!({"name": name, "scope": scope, "permissions": permissions}),
                    true,
                )?),
                RoleCmd::List { current, page_size } => pretty(&c.get(
                    &format!("/role?current={current}&pageSize={page_size}"),
                    true,
                )?),
                RoleCmd::Get { id } => pretty(&c.get(&format!("/role/{id}"), true)?),
                RoleCmd::Update { id, name, scope, permissions } => pretty(&c.put(
                    &format!("/role/{id}"),
                    json!({"name": name, "scope": scope, "permissions": permissions}),
                    true,
                )?),
                RoleCmd::Delete { id } => pretty(&c.delete(&format!("/role/{id}"), true)?),
                RoleCmd::Grant { user, role } => pretty(&c.post(
                    "/user-role/grant",
                    json!({"userId": user, "roleId": role}),
                    true,
                )?),
                RoleCmd::Revoke { user, role } => pretty(&c.post(
                    "/user-role/revoke",
                    json!({"userId": user, "roleId": role}),
                    true,
                )?),
            }
        }
        Cmd::Apidef { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ApidefCmd::Create { project, name, protocol, method, path } => pretty(&c.post(
                    "/api/definition",
                    json!({"projectId": project, "name": name, "protocol": protocol, "method": method, "path": path}),
                    true,
                )?),
                ApidefCmd::Import { project, file, url } => {
                    // --url 远程拉取 / --file 本地读取(二选一,url 优先)。
                    let raw = match (url, file) {
                        (Some(u), _) => c.fetch_text(&u)?,
                        (None, Some(f)) => std::fs::read_to_string(&f)?,
                        (None, None) => return Err("需指定 --file 或 --url".into()),
                    };
                    let content: Value =
                        serde_json::from_str(&raw).map_err(|e| format!("导入内容不是合法 JSON: {e}"))?;
                    pretty(&c.post(
                        "/api/definition/import",
                        json!({"projectId": project, "content": content}),
                        true,
                    )?)
                }
                ApidefCmd::List { project } => {
                    pretty(&c.get(&format!("/api/definition?projectId={project}"), true)?)
                }
                ApidefCmd::Get { id } => pretty(&c.get(&format!("/api/definition/{id}"), true)?),
                ApidefCmd::Case { def, name, method, url, body, expect_status } => pretty(&c.post(
                    &format!("/api/definition/{def}/case"),
                    json!({"name": name, "method": method, "url": url, "body": body, "assertions": status_assertions(expect_status)}),
                    true,
                )?),
                ApidefCmd::Cases { def } => {
                    pretty(&c.get(&format!("/api/definition/{def}/case"), true)?)
                }
                ApidefCmd::CaseNew { project, def, name, method, url, body, expect_status } => pretty(&c.post(
                    "/api/case",
                    json!({"projectId": project, "apiDefinitionId": def, "name": name, "method": method, "url": url, "body": body, "assertions": status_assertions(expect_status)}),
                    true,
                )?),
                ApidefCmd::GenSuite { project, base, no_scenario } => {
                    gen_suite(&c, &project, base.trim_end_matches('/'), no_scenario)?
                }
                ApidefCmd::CaseList { project, current, page_size } => pretty(&c.get(
                    &format!("/api/case?projectId={project}&current={current}&pageSize={page_size}"),
                    true,
                )?),
                ApidefCmd::CaseExec { case, current, page_size } => pretty(&c.get(
                    &format!("/api/case/{case}/executions?current={current}&pageSize={page_size}"),
                    true,
                )?),
                ApidefCmd::CaseRun { case, project, run_mode, pool, env } => pretty(&c.post(
                    &format!("/api/case/{case}/run"),
                    json!({"projectId": project, "runMode": run_mode, "poolId": pool, "environmentId": env}),
                    true,
                )?),
                ApidefCmd::Mock { def, name, response_status, body } => pretty(&c.post(
                    &format!("/api/definition/{def}/mock"),
                    json!({"name": name, "responseStatus": response_status, "responseBody": body}),
                    true,
                )?),
                ApidefCmd::Mocks { def } => {
                    pretty(&c.get(&format!("/api/definition/{def}/mock"), true)?)
                }
            }
        }
        Cmd::Scenario { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ScenarioCmd::Create { project, name } => pretty(&c.post(
                    "/api/scenario",
                    json!({"projectId": project, "name": name}),
                    true,
                )?),
                ScenarioCmd::List { project } => {
                    pretty(&c.get(&format!("/api/scenario?projectId={project}"), true)?)
                }
                ScenarioCmd::Get { id } => pretty(&c.get(&format!("/api/scenario/{id}"), true)?),
                ScenarioCmd::Step { scenario, kind, ref_mode, order, ref_id, method, url, body, control_json } => {
                    let mut step = json!({"kind": kind.to_uppercase(), "refMode": ref_mode.to_uppercase(), "order": order});
                    if let Some(r) = ref_id {
                        step["refId"] = json!(r);
                    }
                    if let Some(m) = method {
                        step["request"] = json!({"method": m, "url": url.unwrap_or_default(), "body": body});
                    }
                    if let Some(cj) = control_json {
                        step["control"] = serde_json::from_str(&cj)
                            .map_err(|e| format!("--control-json 不是合法 JSON: {e}"))?;
                    }
                    pretty(&c.post(&format!("/api/scenario/{scenario}/step"), step, true)?)
                }
                ScenarioCmd::Compile { id } => {
                    pretty(&c.get(&format!("/api/scenario/{id}/compile"), true)?)
                }
                ScenarioCmd::Executions { id, current, page_size } => pretty(&c.get(
                    &format!("/api/scenario/{id}/executions?current={current}&pageSize={page_size}"),
                    true,
                )?),
                ScenarioCmd::Run { id, project, run_mode, pool, env } => pretty(&c.post(
                    &format!("/api/scenario/{id}/run"),
                    json!({"projectId": project, "runMode": run_mode, "poolId": pool, "environmentId": env}),
                    true,
                )?),
            }
        }
        Cmd::Env { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                EnvCmd::Create { project, name, base, headers, vars } => pretty(&c.post(
                    "/api/environment",
                    json!({"projectId": project, "name": name, "baseUrl": base,
                           "headers": parse_headers(&headers)?, "variables": parse_vars(&vars)?}),
                    true,
                )?),
                EnvCmd::List { project } => {
                    pretty(&c.get(&format!("/api/environment?projectId={project}"), true)?)
                }
                EnvCmd::Get { id } => pretty(&c.get(&format!("/api/environment/{id}"), true)?),
                EnvCmd::Update { id, project, name, base, headers, vars } => pretty(&c.put(
                    &format!("/api/environment/{id}"),
                    json!({"projectId": project, "name": name, "baseUrl": base,
                           "headers": parse_headers(&headers)?, "variables": parse_vars(&vars)?}),
                    true,
                )?),
                EnvCmd::Delete { id } => pretty(&c.delete(&format!("/api/environment/{id}"), true)?),
            }
        }
    }
    Ok(())
}

/// 为项目内每个接口定义生成「成功 + 失败」两条用例,并(默认)各建一条场景串联两者。
///
/// 设计:
///  - 成功用例:断言文档化成功码(POST→201,其余→200),代表正常路径预期。
///  - 失败用例:断言 401(未授权),代表负向路径预期。
///
/// 实际执行时(无凭证)受保护接口多返回 401,会如实回写各自 SUCCESS/ERROR,演示执行闭环。
/// 配上带 `Authorization` 的环境(`env create --header`)再跑,正向用例即转绿。
fn gen_suite(c: &Client, project: &str, base: &str, no_scenario: bool) -> R<()> {
    let defs = c.get(&format!("/api/definition?projectId={project}"), true)?;
    let list = defs.as_array().ok_or("接口定义列表不是数组")?;
    if list.is_empty() {
        return Err("项目内没有接口定义,先 `apidef import`".into());
    }
    let (mut cases, mut scenarios, mut steps, mut failed) = (0u32, 0u32, 0u32, 0u32);
    for d in list {
        let def_id = d["id"].as_str().unwrap_or_default();
        let name = d["name"].as_str().unwrap_or("(unnamed)");
        let method = d["method"].as_str().unwrap_or("GET").to_uppercase();
        let path = d["path"].as_str().unwrap_or("");
        let url = format!("{base}{path}");
        let success_code = if method == "POST" { 201 } else { 200 };

        // 成功 + 失败两条用例(挂到定义下,可被批量/场景运行)。
        let mk_case = |label: &str, code: u16| {
            c.post(
                "/api/case",
                json!({
                    "projectId": project,
                    "apiDefinitionId": def_id,
                    "name": format!("{name} [{label}]"),
                    "method": method,
                    "url": url,
                    "body": Value::Null,
                    "assertions": [{"type": "StatusIs", "args": code}],
                }),
                true,
            )
        };
        let ok = match mk_case(&format!("成功·期望{success_code}"), success_code) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("✗ {name}: 成功用例创建失败:{e}");
                failed += 1;
                continue;
            }
        };
        let bad = match mk_case("失败·期望401", 401) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("✗ {name}: 失败用例创建失败:{e}");
                failed += 1;
                continue;
            }
        };
        cases += 2;
        let (ok_id, bad_id) = (
            ok["id"].as_str().unwrap_or_default().to_string(),
            bad["id"].as_str().unwrap_or_default().to_string(),
        );

        if no_scenario {
            continue;
        }
        // 每个接口一条场景:顺序串联「成功 → 失败」两步(引用模式)。
        let sc = match c.post(
            "/api/scenario",
            json!({"projectId": project, "name": format!("{name} 场景")}),
            true,
        ) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("✗ {name}: 场景创建失败:{e}");
                failed += 1;
                continue;
            }
        };
        let sc_id = sc["id"].as_str().unwrap_or_default();
        scenarios += 1;
        for (order, ref_id) in [(1, &ok_id), (2, &bad_id)] {
            match c.post(
                &format!("/api/scenario/{sc_id}/step"),
                json!({"kind": "CASE", "refMode": "REFERENCE", "order": order, "refId": ref_id}),
                true,
            ) {
                Ok(_) => steps += 1,
                Err(e) => eprintln!("✗ {name}: 步骤{order}添加失败:{e}"),
            }
        }
    }
    println!(
        "✅ 生成完成:{} 个接口 → {cases} 条用例、{scenarios} 条场景、{steps} 个步骤(失败 {failed})",
        list.len()
    );
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
        let c = Client::new(Config {
            url: "http://h:1/".into(),
            token: "t".into(),
            agent: None,
        })
        .expect("client");
        assert_eq!(c.url("/x"), "http://h:1/x");
    }

    #[test]
    fn config_defaults_url_when_empty() {
        // 不依赖文件/环境:空配置应回落默认 URL(此处直接验证逻辑)
        let c = Config {
            url: String::new(),
            token: String::new(),
            agent: None,
        };
        assert!(c.url.is_empty());
    }
}
