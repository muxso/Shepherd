//! Remote executor runtime: registers with the server in pull mode with heartbeats,
//! long-polls for WorkSpecs, runs tasks in a git workspace via Claude/generic CLI
//! backends, and reports events and delivery results back.

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
    /// Static API key (SHEPHERD_AGENT_KEY, required): used directly as bearer; no login/password path.
    key: String,
    caps: Vec<String>,
    name: String,
    workdir: String,
    /// Task base ref (e.g. origin/main); unset means the repo's current HEAD.
    /// Pins the task base when host and container share one checkout on different branches.
    base_ref: Option<String>,
    concurrency: usize,
    task_timeout: Duration,
    heartbeat: Duration,
    drain_timeout: Duration,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        let secs = |k: &str, d: u64| -> Duration {
            Duration::from_secs(
                std::env::var(k).ok().and_then(|s| s.parse().ok()).unwrap_or(d).max(1),
            )
        };
        Ok(Self {
            base: env("SHEPHERD_BASE", "http://127.0.0.1:9180"),
            key: agent_key(std::env::var("SHEPHERD_AGENT_KEY").ok())?,
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
        })
    }
}

/// SHEPHERD_AGENT_KEY is the only credential: empty/whitespace counts as unset — fail startup with issuance guidance.
fn agent_key(raw: Option<String>) -> anyhow::Result<String> {
    raw.filter(|s| !s.trim().is_empty()).ok_or_else(|| {
        anyhow::anyhow!(
            "missing SHEPHERD_AGENT_KEY: agent-runtime only accepts API key authentication. \
             Set SHEPHERD_AGENT_KEY=sak_…; a key can be issued from Profile → API KEY or POST /system/apikey"
        )
    })
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
        backend = backend.cli_name(), "claimed task");

    if spec.is_design() {
        match backend.execute(&prompt, base_workdir, &NoopSink).await {
            Ok(doc) => match client.post_design(&spec.attempt_id, &doc).await {
                Ok(()) => {
                    tracing::info!(proposal = %spec.attempt_id, "design draft posted back → pending review")
                }
                Err(e) => {
                    tracing::error!(proposal = %spec.attempt_id, "posting the design draft back finally failed (after retries): {e}")
                }
            },
            Err(e) => tracing::warn!(proposal = %spec.attempt_id, "design drafting failed: {e}"),
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
                        (s.reference, format!("changes: {stat} | {summary}"))
                    }
                    None => (
                        format!("runtime://{}", spec.attempt_id),
                        format!("(no code changes){summary}"),
                    ),
                };
            match client.complete(&spec.attempt_id, "DIFF", &reference, &summary).await {
                Ok(()) => {
                    tracing::info!(attempt = %spec.attempt_id, %reference, "delivery complete")
                }
                Err(e) => tracing::error!(attempt = %spec.attempt_id,
                    "delivery report finally failed (after retries; the server will reclaim on timeout): {e}"),
            }
        }
        Err(e) => {
            if let Err(e2) = client.fail(&spec.attempt_id, &e.to_string()).await {
                tracing::error!(attempt = %spec.attempt_id, "reporting the failure also failed (after retries): {e2} / cause {e}");
            }
        }
    }
    if let Some(p) = wt {
        git::remove_worktree(base_workdir, &p).await;
    }
}

async fn connect(cfg: &Config) -> anyhow::Result<(Arc<ServerClient>, String)> {
    // API key is the sole auth path: no login/refresh; a 401 means the key was revoked.
    let client = Arc::new(ServerClient::with_api_key(&cfg.base, &cfg.key)?);
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

    let cfg = Config::from_env()?;
    let mc = cfg.concurrency as u32;
    let mut sd = install_shutdown();

    let (client, id0) = loop {
        if *sd.borrow_and_update() {
            return Ok(());
        }
        match connect(&cfg).await {
            Ok(pair) => break pair,
            Err(e) => {
                tracing::warn!(base = %cfg.base, "failed to connect to the server, retrying in 5s: {e}");
                tokio::select! {
                    _ = wait_shutdown(&mut sd) => return Ok(()),
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }
            }
        }
    };
    let runtime_id = Arc::new(Mutex::new(id0.clone()));
    tracing::info!(base = %cfg.base, caps = ?cfg.caps, concurrency = cfg.concurrency, runtime_id = %id0,
        "agent-runtime online");

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
                tracing::warn!("claim failed: {e}");
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
            "shutdown signal received, waiting for in-flight tasks to finish…"
        );
        match tokio::time::timeout(cfg.drain_timeout, sem.acquire_many(mc)).await {
            Ok(_) => tracing::info!("all in-flight tasks finished"),
            Err(_) => tracing::warn!(
                stuck = cfg.concurrency - sem.available_permits(),
                "drain timed out, forcing exit (the server will reclaim unfinished tasks on timeout)"
            ),
        }
    }
    tracing::info!("agent-runtime shut down gracefully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_key_accepts_real_key() {
        assert_eq!(agent_key(Some("sak_ab.cd".into())).expect("key"), "sak_ab.cd");
    }

    #[test]
    fn agent_key_missing_or_blank_errors_with_guidance() {
        for raw in [None, Some(String::new()), Some("   ".into())] {
            let err = agent_key(raw).expect_err("blank/missing key must fail startup");
            let msg = err.to_string();
            assert!(msg.contains("SHEPHERD_AGENT_KEY"), "must name the missing env var: {msg}");
            assert!(msg.contains("/system/apikey"), "must give issuance guidance: {msg}");
        }
    }
}
