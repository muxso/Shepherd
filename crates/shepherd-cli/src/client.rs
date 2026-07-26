//! Shared client, configuration, and output helpers for the Shepherd CLI.
//!
//! Every command module uses these building blocks: [`Client`] talks to the server,
//! [`Config`] loads/saves the local credentials, and the `pretty`/`render_*` family
//! turns server JSON into human-readable tables (or raw JSON when `--json` is set).

use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

pub use serde_json::{json, Value};

pub type R<T> = Result<T, Box<dyn Error>>;

pub static JSON_OUTPUT: AtomicBool = AtomicBool::new(false);

pub const NO_KEY_HINT: &str =
    "未配置 API key:执行 `shepherd login --api-key sak_…` 或设 SHEPHERD_API_KEY\
(关键可在 个人中心 → API KEY 或 POST /system/apikey 签发)";

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Config {
    /// Server base URL.
    pub url: String,
    /// Static API key (sak_…).
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub agent: Option<String>,
}

pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".shepherd").join("config.json")
}

impl Config {
    pub fn load() -> Config {
        let mut c: Config = std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if let Ok(u) = std::env::var("SHEPHERD_URL") {
            c.url = u;
        }
        if let Ok(k) = std::env::var("SHEPHERD_API_KEY") {
            c.api_key = k;
        }
        if c.url.is_empty() {
            c.url = "http://127.0.0.1:8088".into();
        }
        c
    }

    pub fn save(&self) -> R<()> {
        let p = config_path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&p, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

pub struct Client {
    http: reqwest::blocking::Client,
    cfg: Config,
}

impl Client {
    pub fn new(cfg: Config) -> R<Client> {
        // no_proxy: the server is usually local/intranet; avoid interception by a global proxy.
        let http = reqwest::blocking::Client::builder().no_proxy().build()?;
        Ok(Client { http, cfg })
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.cfg.url.trim_end_matches('/'), path)
    }

    fn send(&self, mut rb: reqwest::blocking::RequestBuilder, auth: bool) -> R<Value> {
        if auth {
            if self.cfg.api_key.is_empty() {
                return Err(NO_KEY_HINT.into());
            }
            rb = rb.bearer_auth(&self.cfg.api_key);
        }
        let resp = rb.send()?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {text}").into());
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    pub fn post(&self, path: &str, body: Value, auth: bool) -> R<Value> {
        self.send(self.http.post(self.url(path)).json(&body), auth)
    }
    pub fn get(&self, path: &str, auth: bool) -> R<Value> {
        self.send(self.http.get(self.url(path)), auth)
    }
    pub fn post_bytes(&self, path: &str, bytes: Vec<u8>, auth: bool) -> R<Value> {
        self.send(
            self.http
                .post(self.url(path))
                .header("content-type", "application/octet-stream")
                .body(bytes),
            auth,
        )
    }
    pub fn get_bytes(&self, path: &str, auth: bool) -> R<Vec<u8>> {
        let mut rb = self.http.get(self.url(path));
        if auth {
            if self.cfg.api_key.is_empty() {
                return Err(NO_KEY_HINT.into());
            }
            rb = rb.bearer_auth(&self.cfg.api_key);
        }
        let resp = rb.send()?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {}", resp.text().unwrap_or_default()).into());
        }
        Ok(resp.bytes()?.to_vec())
    }
    pub fn get_text(&self, path: &str, auth: bool) -> R<String> {
        let mut rb = self.http.get(self.url(path));
        if auth {
            if self.cfg.api_key.is_empty() {
                return Err(NO_KEY_HINT.into());
            }
            rb = rb.bearer_auth(&self.cfg.api_key);
        }
        let resp = rb.send()?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {text}").into());
        }
        Ok(text)
    }
    pub fn put(&self, path: &str, body: Value, auth: bool) -> R<Value> {
        self.send(self.http.put(self.url(path)).json(&body), auth)
    }
    pub fn fetch_text(&self, url: &str) -> R<String> {
        let resp = self.http.get(url).send()?;
        if !resp.status().is_success() {
            return Err(format!("拉取 {url} 失败:HTTP {}", resp.status()).into());
        }
        Ok(resp.text()?)
    }
    pub fn delete(&self, path: &str, auth: bool) -> R<Value> {
        self.send(self.http.delete(self.url(path)), auth)
    }
    pub fn delete_body(&self, path: &str, body: Value, auth: bool) -> R<Value> {
        self.send(self.http.delete(self.url(path)).json(&body), auth)
    }
}

pub fn pretty(v: &Value) {
    if JSON_OUTPUT.load(std::sync::atomic::Ordering::Relaxed) {
        println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
        return;
    }
    render_human(v);
}

/// Print a freshly-minted API key (returned only once) before the normal output.
pub fn print_key(v: &Value) {
    if let Some(k) = v.get("key").and_then(|x| x.as_str()) {
        println!(" 已创建,密钥(仅此一次可见):\n{k}");
    }
    pretty(v);
}

pub fn cell(v: &Value) -> String {
    match v {
        Value::Null => "—".to_string(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(a) => format!("[{} 项]", a.len()),
        Value::Object(o) => format!("{{{} 字段}}", o.len()),
    }
}

pub fn render_human(v: &Value) {
    if let Some(items) = v.get("items").and_then(|x| x.as_array()) {
        render_table(items);
        let total = v.get("total").map(cell).unwrap_or_default();
        let cur = v.get("current").map(cell).unwrap_or_default();
        let pages = v.get("totalPages").map(cell).unwrap_or_default();
        if !total.is_empty() {
            println!("\n共 {total} 条 · 第 {cur} 页 / 共 {pages} 页");
        }
        return;
    }
    match v {
        Value::Array(a) => render_table(a),
        Value::Object(_) => render_kv(v),
        other => println!("{}", cell(other)),
    }
}

pub fn render_table(items: &[Value]) {
    if items.is_empty() {
        println!("(空)");
        return;
    }
    let mut cols: Vec<String> = Vec::new();
    for it in items {
        if let Some(o) = it.as_object() {
            for k in o.keys() {
                if !cols.contains(k) {
                    cols.push(k.clone());
                }
            }
        }
    }
    if cols.is_empty() {
        for it in items {
            println!("{}", cell(it));
        }
        return;
    }
    let trunc = |s: String| {
        if s.chars().count() > 40 {
            format!("{}…", s.chars().take(39).collect::<String>())
        } else {
            s
        }
    };
    let mut widths: Vec<usize> = cols.iter().map(|c| c.chars().count()).collect();
    let rows: Vec<Vec<String>> = items
        .iter()
        .map(|it| {
            cols.iter()
                .map(|c| trunc(it.get(c).map(cell).unwrap_or_else(|| "—".to_string())))
                .collect()
        })
        .collect();
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count());
        }
    }
    let pad = |s: &str, w: usize| {
        let n = s.chars().count();
        format!("{}{}", s, " ".repeat(w.saturating_sub(n)))
    };
    let header: Vec<String> = cols.iter().enumerate().map(|(i, c)| pad(c, widths[i])).collect();
    println!("{}", header.join("  "));
    println!("{}", widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>().join("  "));
    for r in &rows {
        let line: Vec<String> = r.iter().enumerate().map(|(i, c)| pad(c, widths[i])).collect();
        println!("{}", line.join("  "));
    }
}

pub fn render_kv(v: &Value) {
    if let Some(o) = v.as_object() {
        let w = o.keys().map(|k| k.chars().count()).max().unwrap_or(0);
        for (k, val) in o {
            let pad = " ".repeat(w.saturating_sub(k.chars().count()));
            println!("{k}{pad} : {}", cell(val));
        }
    }
}

pub fn status_assertions(expect_status: Option<u16>) -> Value {
    match expect_status {
        Some(code) => json!([{ "type": "StatusIs", "args": code }]),
        None => json!([]),
    }
}

pub fn parse_headers(items: &[String]) -> R<Value> {
    let mut arr = Vec::with_capacity(items.len());
    for it in items {
        let (name, value) =
            it.split_once(':').ok_or_else(|| format!("--header 需 'Name: value' 格式:{it}"))?;
        arr.push(json!({"name": name.trim(), "value": value.trim()}));
    }
    Ok(Value::Array(arr))
}

pub fn parse_vars(items: &[String]) -> R<Value> {
    let mut map = serde_json::Map::with_capacity(items.len());
    for it in items {
        let (k, v) = it.split_once('=').ok_or_else(|| format!("--var 需 'key=value' 格式:{it}"))?;
        map.insert(k.trim().to_string(), json!(v));
    }
    Ok(Value::Object(map))
}

const TPL_REQUIREMENT: &str = "# 需求模板

标题: 用户登录
描述: 支持邮箱 + 密码登录,签发会话令牌。

验收标准:
- 正确凭证返回 token
- 错误凭证返回 401
- 令牌过期后需重新登录

改好后录入(需先 `shepherd login`):

    shepherd req add --project <projectId> \\
      --title \"用户登录\" \\
      --description \"支持邮箱 + 密码登录,签发会话令牌。\" \\
      --criteria \"正确凭证返回 token,错误凭证返回 401,令牌过期后需重新登录\"
";

const TPL_GETTING_STARTED: &str = "# Shepherd 上手

前置:一个运行中的 server(默认 http://localhost:8088)。

1. 配置认证:`shepherd login --url http://localhost:8088 --api-key sak_…`
   (API key 在 个人中心 → API KEY 或 `POST /system/apikey` 签发;也可设环境变量 SHEPHERD_API_KEY)
2. 录入需求:见 `requirements/example.md`
3. 拆分任务:`shepherd decompose --req <requirementId> --version 1`
4. 派发执行:`shepherd dispatch --decomp <decompositionId> --task <taskId> --executor CLAUDE_CODE`
5. 验证 / 复查:`shepherd verify --help`、`shepherd decomposition --help`

各命令的完整参数见 `shepherd <命令> --help`。
";

/// Scaffold file manifest (relative path -> contents). Pure function for testability.
pub fn scaffold_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("requirements/example.md", TPL_REQUIREMENT),
        ("shepherd.getting-started.md", TPL_GETTING_STARTED),
    ]
}

pub fn normalize_agent(t: &str) -> R<String> {
    match t.to_ascii_lowercase().replace('-', "_").as_str() {
        "claude_code" => Ok("CLAUDE_CODE".into()),
        "codex" => Ok("CODEX".into()),
        "opencode" => Ok("OPENCODE".into()),
        // The brand is one word, but the claude-code spelling invites code-buddy; accept both.
        "codebuddy" | "code_buddy" => Ok("CODEBUDDY".into()),
        other => Err(format!(
            "未知 agent 类型: {other}(支持 claude-code | codex | opencode | codebuddy)"
        )
        .into()),
    }
}

pub fn gen_suite(c: &Client, project: &str, base: &str, no_scenario: bool) -> R<()> {
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
                eprintln!(" {name}: 成功用例创建失败:{e}");
                failed += 1;
                continue;
            }
        };
        let bad = match mk_case("失败·期望401", 401) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(" {name}: 失败用例创建失败:{e}");
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
        let sc = match c.post(
            "/api/scenario",
            json!({"projectId": project, "name": format!("{name} 场景")}),
            true,
        ) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(" {name}: 场景创建失败:{e}");
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
                Err(e) => eprintln!(" {name}: 步骤{order}添加失败:{e}"),
            }
        }
    }
    println!(
        " 生成完成:{} 个接口 → {cases} 条用例、{scenarios} 条场景、{steps} 个步骤(失败 {failed})",
        list.len()
    );
    Ok(())
}
