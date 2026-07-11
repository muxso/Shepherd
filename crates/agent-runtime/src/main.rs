//! 远程执行者运行时:以 pull 模式向 server 注册并心跳,长轮询领取 WorkSpec,
//! 调用 Claude/通用 CLI 等后端在 git 工作区内执行任务,回传事件与交付结果。

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
    /// 静态 API key(SHEPHERD_AGENT_KEY):设了就直接当 bearer 用,不再走登录/口令。
    key: Option<String>,
    user: String,
    pass: String,
    caps: Vec<String>,
    name: String,
    workdir: String,
    /// 任务基点 ref(如 origin/main);不设则用仓库当前 HEAD。
    /// 宿主机与容器共用一个检出、各在不同分支时,用它把任务基点钉住。
    base_ref: Option<String>,
    concurrency: usize,
    task_timeout: Duration,
    heartbeat: Duration,
    drain_timeout: Duration,
}

impl Config {
    fn from_env() -> Self {
        let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        let secs = |k: &str, d: u64| -> Duration {
            Duration::from_secs(
                std::env::var(k).ok().and_then(|s| s.parse().ok()).unwrap_or(d).max(1),
            )
        };
        Self {
            base: env("SHEPHERD_BASE", "http://127.0.0.1:9180"),
            key: agent_key(std::env::var("SHEPHERD_AGENT_KEY").ok()),
            user: env("SHEPHERD_ADMIN_USER", "admin"),
            pass: env("SHEPHERD_ADMIN_PASSWORD", "s3cret"),
            caps: env("SHEPHERD_CAPS", "CLAUDE_CODE")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            name: env("RUNTIME_NAME", "agent-runtime"),
            workdir: env("AGENT_WORKDIR", "."),
            base_ref: std::env::var("AGENT_BASE_REF").ok().filter(|s| !s.trim().is_empty()),
            concurrency: env("AGENT_CONCURRENCY", "1").parse().unwrap_or(1).max(1),
            task_timeout: secs("AGENT_TASK_TIMEOUT_SECS", 1800),
            heartbeat: secs("AGENT_HEARTBEAT_SECS", 10),
            drain_timeout: secs("AGENT_DRAIN_TIMEOUT_SECS", 60),
        }
    }

    /// 弱默认口令只在口令真正用于登录时才值得告警;配了 API key 后口令不会被使用。
    fn warns_weak_password(&self) -> bool {
        self.key.is_none() && matches!(self.pass.as_str(), "admin" | "change-me" | "s3cret")
    }
}

/// SHEPHERD_AGENT_KEY:空串/纯空白视同未设置(回落到口令登录)。
fn agent_key(raw: Option<String>) -> Option<String> {
    raw.filter(|s| !s.trim().is_empty())
}

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

async fn wait_shutdown(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow_and_update() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

fn backend_for(executor: &str, task_timeout: Duration) -> Arc<dyn CliAgentBackend> {
    if std::env::var("AGENT_MOCK").is_ok() {
        return Arc::new(MockBackend::default());
    }
    match executor {
        "CODEX" => Arc::new(GenericCliBackend::codex(task_timeout)),
        "OPENCODE" => Arc::new(GenericCliBackend::opencode(task_timeout)),
        "CODEBUDDY" => Arc::new(GenericCliBackend::codebuddy(task_timeout)),
        _ => Arc::new(ClaudeBackend::new(task_timeout)),
    }
}

async fn handle(
    client: &ServerClient,
    spec: &WorkSpec,
    base_workdir: &str,
    base_ref: Option<&str>,
    task_timeout: Duration,
) {
    let backend = backend_for(&spec.executor, task_timeout);
    let prompt = spec.to_prompt();
    let mode = if spec.is_design() { "design" } else { "implement" };
    tracing::info!(attempt = %spec.attempt_id, executor = %spec.executor, mode,
        backend = backend.cli_name(), "认领任务");

    if spec.is_design() {
        match backend.execute(&prompt, base_workdir, &NoopSink).await {
            Ok(doc) => match client.post_design(&spec.attempt_id, &doc).await {
                Ok(()) => tracing::info!(proposal = %spec.attempt_id, "设计稿已回填 → 待审"),
                Err(e) => {
                    tracing::error!(proposal = %spec.attempt_id, "回填设计稿最终失败(已重试): {e}")
                }
            },
            Err(e) => tracing::warn!(proposal = %spec.attempt_id, "设计起草失败: {e}"),
        }
        return;
    }

    let wt = git::add_worktree(base_workdir, &spec.attempt_id, base_ref).await;
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
            match client.complete(&spec.attempt_id, "DIFF", &reference, &summary).await {
                Ok(()) => tracing::info!(attempt = %spec.attempt_id, %reference, "交付完成"),
                Err(e) => tracing::error!(attempt = %spec.attempt_id,
                    "交付上报最终失败(已重试,server 将到点 reclaim): {e}"),
            }
        }
        Err(e) => {
            if let Err(e2) = client.fail(&spec.attempt_id, &e.to_string()).await {
                tracing::error!(attempt = %spec.attempt_id, "失败上报也失败(已重试): {e2} / 原因 {e}");
            }
        }
    }
    if let Some(p) = wt {
        git::remove_worktree(base_workdir, &p).await;
    }
}

async fn connect(cfg: &Config) -> anyhow::Result<(Arc<ServerClient>, String)> {
    // API key 优先:免登录免刷新;未配 key 才退回管理员口令登录。
    let client = match cfg.key.as_deref() {
        Some(key) => Arc::new(ServerClient::with_api_key(&cfg.base, key)?),
        None => Arc::new(ServerClient::login(&cfg.base, &cfg.user, &cfg.pass).await?),
    };
    let id = client.register(&cfg.name, &cfg.caps, cfg.concurrency as u32).await?;
    Ok((client, id))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cfg = Config::from_env();
    // 弱默认口令警告:runtime 与 server 共用登录凭证,默认值等于把认领接口敞开。
    if cfg.warns_weak_password() {
        tracing::warn!(
            "SHEPHERD_ADMIN_PASSWORD 使用弱默认值;生产部署应改为强随机口令,或改用 SHEPHERD_AGENT_KEY(推荐)"
        );
    }
    let mc = cfg.concurrency as u32;
    let mut sd = install_shutdown();

    let (client, id0) = loop {
        if *sd.borrow_and_update() {
            return Ok(());
        }
        match connect(&cfg).await {
            Ok(pair) => break pair,
            Err(e) => {
                tracing::warn!(base = %cfg.base, "连接 server 失败,5s 后重试: {e}");
                tokio::select! {
                    _ = wait_shutdown(&mut sd) => return Ok(()),
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }
            }
        }
    };
    let runtime_id = Arc::new(Mutex::new(id0.clone()));
    tracing::info!(base = %cfg.base, caps = ?cfg.caps, concurrency = cfg.concurrency, runtime_id = %id0,
        "agent-runtime 上线");

    let hb = {
        let (client, rid, name, caps, every) =
            (client.clone(), runtime_id.clone(), cfg.name.clone(), cfg.caps.clone(), cfg.heartbeat);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(every).await;
                let cur = rid.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
                if !client.heartbeat(&cur).await.unwrap_or(false) {
                    if let Ok(new) = client.register(&name, &caps, mc).await {
                        *rid.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = new;
                    }
                }
            }
        })
    };

    let sem = Arc::new(Semaphore::new(cfg.concurrency));

    loop {
        if *sd.borrow_and_update() {
            break;
        }
        let permit = tokio::select! {
            _ = wait_shutdown(&mut sd) => break,
            p = sem.clone().acquire_owned() => p.expect("semaphore closed"),
        };
        let rid_now = runtime_id.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        let claimed = tokio::select! {
            _ = wait_shutdown(&mut sd) => { drop(permit); break; }
            r = client.claim(&cfg.caps, &rid_now) => r,
        };
        match claimed {
            Ok(Some(spec)) => {
                let (client, wd, br, tt) =
                    (client.clone(), cfg.workdir.clone(), cfg.base_ref.clone(), cfg.task_timeout);
                tokio::spawn(async move {
                    handle(&client, &spec, &wd, br.as_deref(), tt).await;
                    drop(permit);
                });
            }
            Ok(None) => drop(permit),
            Err(e) => {
                tracing::warn!("认领出错: {e}");
                drop(permit);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    hb.abort();
    let inflight = cfg.concurrency - sem.available_permits();
    if inflight > 0 {
        tracing::info!(
            inflight,
            drain_secs = cfg.drain_timeout.as_secs(),
            "收到关停信号,等待在飞任务收尾…"
        );
        match tokio::time::timeout(cfg.drain_timeout, sem.acquire_many(mc)).await {
            Ok(_) => tracing::info!("在飞任务已全部收尾"),
            Err(_) => tracing::warn!(
                stuck = cfg.concurrency - sem.available_permits(),
                "排空超时,强制退出(server 将到点 reclaim 未完成任务)"
            ),
        }
    }
    tracing::info!("agent-runtime 优雅退出");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(key: Option<&str>, pass: &str) -> Config {
        Config {
            base: "http://127.0.0.1:9180".into(),
            key: agent_key(key.map(str::to_string)),
            user: "admin".into(),
            pass: pass.into(),
            caps: vec!["CLAUDE_CODE".into()],
            name: "t".into(),
            workdir: ".".into(),
            base_ref: None,
            concurrency: 1,
            task_timeout: Duration::from_secs(1),
            heartbeat: Duration::from_secs(1),
            drain_timeout: Duration::from_secs(1),
        }
    }

    #[test]
    fn agent_key_blank_means_unset() {
        assert_eq!(agent_key(None), None);
        assert_eq!(agent_key(Some(String::new())), None);
        assert_eq!(agent_key(Some("   ".into())), None);
        assert_eq!(agent_key(Some("sak_ab.cd".into())), Some("sak_ab.cd".to_string()));
    }

    #[test]
    fn key_set_wins_over_password_login() {
        // key 存在即走静态模式;弱口令告警随之关闭(口令不会被使用)。
        let c = cfg(Some("sak_ab.cd"), "s3cret");
        assert_eq!(c.key.as_deref(), Some("sak_ab.cd"));
        assert!(!c.warns_weak_password());
    }

    #[test]
    fn blank_key_falls_back_to_password_login() {
        let c = cfg(Some("  "), "s3cret");
        assert_eq!(c.key, None);
        assert!(c.warns_weak_password());
    }

    #[test]
    fn weak_default_password_warns_without_key() {
        for weak in ["admin", "change-me", "s3cret"] {
            assert!(cfg(None, weak).warns_weak_password(), "{weak} 应触发告警");
        }
        assert!(!cfg(None, "Xq9!rand0m").warns_weak_password());
    }
}
