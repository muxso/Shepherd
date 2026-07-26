//! Top-level commands that are not wrapped in a subcommand enum:
//! `login`, `init`, `logout`, `decompose`, `dispatch`, `metrics`.

use std::path::Path;

use crate::client::*;

pub fn run_init(dir: String, force: bool) -> R<()> {
    let root = Path::new(&dir);
    for (rel, contents) in scaffold_files() {
        let path = root.join(rel);
        if path.exists() && !force {
            return Err(format!("已存在 {}(加 --force 覆盖)", path.display()).into());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, contents)?;
        println!("写入 {}", path.display());
    }
    println!("下一步:编辑 requirements/example.md,然后 `shepherd login` 并按其中命令录入需求。");
    Ok(())
}

pub fn run_login(url: String, api_key: Option<String>) -> R<()> {
    let mut cfg = Config::load();
    cfg.url = url;
    let key = api_key
        .or_else(|| std::env::var("SHEPHERD_API_KEY").ok())
        .filter(|k| !k.trim().is_empty())
        .ok_or(NO_KEY_HINT)?;
    cfg.api_key = key.trim().to_string();
    // The key is a static credential with no login endpoint to validate; only probe reachability — auth errors surface on the first business command.
    let healthy = Client::new(cfg.clone())?.get("/healthz", false).is_ok();
    cfg.save()?;
    println!(
        " 已保存 {} 的 API key → {} 服务{}",
        cfg.url,
        config_path().display(),
        if healthy { "可达" } else { "暂不可达" }
    );
    Ok(())
}

pub fn run_logout() -> R<()> {
    let mut cfg = Config::load();
    cfg.api_key.clear();
    cfg.save()?;
    println!(" 已清除本地 API key(要让 key 失效,请在服务端 API KEY 管理里吊销)");
    Ok(())
}

pub fn run_decompose(req: String, version: u32) -> R<()> {
    let c = Client::new(Config::load())?;
    pretty(&c.post(
        "/decomposition",
        json!({"requirementId": req, "requirementVersion": version}),
        true,
    )?);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run_dispatch(
    decomp: String,
    task: String,
    title: String,
    executor: Option<String>,
    instructions: Option<String>,
    project: Option<String>,
    skills: Vec<String>,
) -> R<()> {
    let cfg = Config::load();
    let exec = executor.or_else(|| cfg.agent.clone()).unwrap_or_else(|| "CLAUDE_CODE".into());
    let c = Client::new(cfg)?;
    let mut instr = instructions;
    if !skills.is_empty() {
        let project = project.ok_or("--skills 需配合 --project")?;
        let comp =
            c.post("/skill/compose", json!({"projectId": project, "skillIds": skills}), true)?;
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
    Ok(())
}

pub fn run_metrics() -> R<()> {
    let c = Client::new(Config::load())?;
    print!("{}", c.get_text("/metrics", false)?);
    Ok(())
}
