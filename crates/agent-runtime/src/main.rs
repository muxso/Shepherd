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

use std::sync::Arc;
use std::time::Duration;

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

async fn handle(client: &ServerClient, spec: &WorkSpec, workdir: &str) {
    let backend = backend_for(&spec.executor);
    let prompt = spec.to_prompt();
    tracing::info!(attempt = %spec.attempt_id, executor = %spec.executor,
        mode = if spec.is_design() { "design" } else { "implement" }, backend = backend.cli_name(),
        "认领任务");

    // design 模式提案无事件端点 → NoopSink;实现模式实时回流到 /delivery/{id}/events。
    let result = if spec.is_design() {
        backend.execute(&prompt, &NoopSink).await
    } else {
        let sink = HttpSink { client, attempt_id: spec.attempt_id.clone() };
        backend.execute(&prompt, &sink).await
    };

    match result {
        Ok(output) => {
            if spec.is_design() {
                match client.post_design(&spec.attempt_id, &output).await {
                    Ok(()) => tracing::info!(proposal = %spec.attempt_id, "设计稿已回填 → 待审"),
                    Err(e) => tracing::warn!("回填设计稿失败: {e}"),
                }
            } else {
                // 实现模式:把工作区改动快照成 commit 作交付物;无改动则占位。
                let summary: String = output.chars().take(700).collect();
                let (reference, summary) = match git::snapshot(workdir, &spec.attempt_id, &spec.title).await {
                    Some(s) => {
                        let stat: String = s.stat.replace('\n', ";").chars().take(300).collect();
                        (s.reference, format!("变更:{stat} | {summary}"))
                    }
                    None => (format!("runtime://{}", spec.attempt_id), format!("(无代码变动){summary}")),
                };
                client.complete(&spec.attempt_id, "DIFF", &reference, &summary).await;
                tracing::info!(attempt = %spec.attempt_id, %reference, "交付完成");
            }
        }
        Err(e) => {
            if spec.is_design() {
                tracing::warn!("设计起草失败: {e}");
            } else {
                client.fail(&spec.attempt_id, &e.to_string()).await;
            }
        }
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
    let client = ServerClient::login(&cfg.base, &cfg.user, &cfg.pass).await?;
    let mut runtime_id = client.register(&cfg.name, &cfg.caps).await?;
    tracing::info!(base = %cfg.base, caps = ?cfg.caps, %runtime_id, "agent-runtime 上线");

    loop {
        if !client.heartbeat(&runtime_id).await.unwrap_or(false) {
            // 心跳 404 → 重新注册。
            if let Ok(id) = client.register(&cfg.name, &cfg.caps).await {
                runtime_id = id;
            }
        }
        match client.claim(&cfg.caps, &runtime_id).await {
            Ok(Some(spec)) => handle(&client, &spec, &cfg.workdir).await,
            Ok(None) => {} // 204:无活,继续长轮询
            Err(e) => {
                tracing::warn!("认领出错: {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}
