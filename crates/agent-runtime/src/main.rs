//! Shepherd 执行者 runtime(纯 Rust)。出站认领 → CLI 后端执行 → 按模式回调。
//!
//! step 1:骨架 + Mock 后端(可编译、可跑、单测绿)。step 2 接 claude/codex/opencode + git 快照。
//!
//! 环境变量:
//!   SHEPHERD_BASE(默认 http://127.0.0.1:9180)、MS_ADMIN_USER/MS_ADMIN_PASSWORD、
//!   SHEPHERD_CAPS(默认 CLAUDE_CODE)、RUNTIME_NAME、AGENT_MOCK(=1 用 mock 后端)。

// 保留完整 claim 字段(decomposition_id/task_id 暂未被 runtime 用到,留作对账/未来用)。
#![allow(dead_code)]

mod backend;
mod client;
mod events;
mod git;
mod models;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{watch, Semaphore};

use backend::{ClaudeBackend, CliAgentBackend, GenericCliBackend, MockBackend};
use client::{HttpSink, ServerClient};
use events::NoopSink;
use models::WorkSpec;

struct Config {
    base: String,
    user: String,
    pass: String,
    caps: Vec<String>,
    name: String,
    /// 实现模式下 agent 改文件 + git 快照的工作目录。
    workdir: String,
    /// 并发认领上限:同时在飞的任务数(信号量)。
    concurrency: usize,
}

impl Config {
    fn from_env() -> Self {
        let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        Self {
            base: env("SHEPHERD_BASE", "http://127.0.0.1:9180"),
            user: env("MS_ADMIN_USER", "admin"),
            pass: env("MS_ADMIN_PASSWORD", "s3cret"),
            caps: env("SHEPHERD_CAPS", "CLAUDE_CODE")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            name: env("RUNTIME_NAME", "agent-runtime"),
            workdir: env("AGENT_WORKDIR", "."),
            concurrency: env("AGENT_CONCURRENCY", "1").parse().unwrap_or(1).max(1),
        }
    }
}

/// 安装关停信号(SIGINT/SIGTERM)监听,返回一个 watch 接收端:值变 true 即应退出。
fn install_shutdown() -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(false);
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            match signal(SignalKind::terminate()) {
                Ok(mut term) => {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {}
                        _ = term.recv() => {}
                    }
                }
                Err(_) => {
                    let _ = tokio::signal::ctrl_c().await;
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        let _ = tx.send(true);
    });
    rx
}

/// 等待关停(值变 true);select 中败者被取消,不会误吞信号。
async fn wait_shutdown(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow_and_update() {
            return;
        }
        if rx.changed().await.is_err() {
            return; // 发送端已 drop
        }
    }
}

/// 选择后端:`AGENT_MOCK=1` → mock;否则按 executor 选 claude/codex/opencode。
fn backend_for(executor: &str) -> Arc<dyn CliAgentBackend> {
    if std::env::var("AGENT_MOCK").is_ok() {
        return Arc::new(MockBackend::default());
    }
    match executor {
        "CODEX" => Arc::new(GenericCliBackend::codex()),
        "OPENCODE" => Arc::new(GenericCliBackend::opencode()),
        _ => Arc::new(ClaudeBackend::default()),
    }
}

async fn handle(client: &ServerClient, spec: &WorkSpec, base_workdir: &str) {
    let backend = backend_for(&spec.executor);
    let prompt = spec.to_prompt();
    let mode = if spec.is_design() { "design" } else { "implement" };
    tracing::info!(attempt = %spec.attempt_id, executor = %spec.executor, mode,
        backend = backend.cli_name(), "认领任务");

    if spec.is_design() {
        // 设计模式:产文档、不改文件、不提交 → 在 base workdir 跑即可,事件无端点(NoopSink)。
        match backend.execute(&prompt, base_workdir, &NoopSink).await {
            Ok(doc) => match client.post_design(&spec.attempt_id, &doc).await {
                Ok(()) => tracing::info!(proposal = %spec.attempt_id, "设计稿已回填 → 待审"),
                Err(e) => tracing::warn!("回填设计稿失败: {e}"),
            },
            Err(e) => tracing::warn!("设计起草失败: {e}"),
        }
        return;
    }

    // 实现模式:每任务独立 worktree(并发安全、不污染 base);worktree 建失败则回退 base。
    let wt = git::add_worktree(base_workdir, &spec.attempt_id).await;
    let run_dir = wt.as_deref().unwrap_or(base_workdir);
    let sink = HttpSink { client, attempt_id: spec.attempt_id.clone() };
    match backend.execute(&prompt, run_dir, &sink).await {
        Ok(output) => {
            let summary: String = output.chars().take(700).collect();
            let (reference, summary) =
                match git::snapshot(run_dir, &spec.attempt_id, &spec.title).await {
                    Some(s) => {
                        let stat: String = s.stat.replace('\n', ";").chars().take(300).collect();
                        (s.reference, format!("变更:{stat} | {summary}"))
                    }
                    None => {
                        (format!("runtime://{}", spec.attempt_id), format!("(无代码变动){summary}"))
                    }
                };
            client.complete(&spec.attempt_id, "DIFF", &reference, &summary).await;
            tracing::info!(attempt = %spec.attempt_id, %reference, "交付完成");
        }
        Err(e) => client.fail(&spec.attempt_id, &e.to_string()).await,
    }
    if let Some(p) = wt {
        git::remove_worktree(base_workdir, &p).await;
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cfg = Config::from_env();
    let client = Arc::new(ServerClient::login(&cfg.base, &cfg.user, &cfg.pass).await?);
    let mc = cfg.concurrency as u32;
    let id0 = client.register(&cfg.name, &cfg.caps, mc).await?;
    let runtime_id = Arc::new(Mutex::new(id0.clone()));
    tracing::info!(base = %cfg.base, caps = ?cfg.caps, concurrency = cfg.concurrency, runtime_id = %id0,
        "agent-runtime 上线");

    // 心跳后台任务(与认领解耦,长任务不饿死心跳);404 重注册并更新共享 id。
    let hb = {
        let (client, rid, name, caps) =
            (client.clone(), runtime_id.clone(), cfg.name.clone(), cfg.caps.clone());
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let cur = rid.lock().unwrap().clone();
                if !client.heartbeat(&cur).await.unwrap_or(false) {
                    if let Ok(new) = client.register(&name, &caps, mc).await {
                        *rid.lock().unwrap() = new;
                    }
                }
            }
        })
    };

    // 并发认领:信号量限同时在飞任务数;每认领一个 spawn 一个 handler(完成释放槽)。
    let sem = Arc::new(Semaphore::new(cfg.concurrency));
    let mut sd = install_shutdown();

    loop {
        if *sd.borrow_and_update() {
            break;
        }
        // 等空闲槽(达上限则阻塞)或关停。
        let permit = tokio::select! {
            _ = wait_shutdown(&mut sd) => break,
            p = sem.clone().acquire_owned() => p.expect("semaphore closed"),
        };
        let rid_now = runtime_id.lock().unwrap().clone();
        // 长轮询认领或关停(关停时释放槽并退出)。
        let claimed = tokio::select! {
            _ = wait_shutdown(&mut sd) => { drop(permit); break; }
            r = client.claim(&cfg.caps, &rid_now) => r,
        };
        match claimed {
            Ok(Some(spec)) => {
                let (client, wd) = (client.clone(), cfg.workdir.clone());
                tokio::spawn(async move {
                    handle(&client, &spec, &wd).await;
                    drop(permit); // 完成释放并发槽
                });
            }
            Ok(None) => drop(permit), // 204:无活
            Err(e) => {
                tracing::warn!("认领出错: {e}");
                drop(permit);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    // 优雅退出:停心跳,等所有在飞任务收尾(获满全部许可 = 无在飞)。
    hb.abort();
    let inflight = cfg.concurrency - sem.available_permits();
    tracing::info!(inflight, "收到关停信号,等待在飞任务收尾…");
    let _ = sem.acquire_many(cfg.concurrency as u32).await;
    tracing::info!("agent-runtime 优雅退出");
    Ok(())
}
