//! Shepherd 执行者 runtime(纯 Rust)。出站认领 → CLI 后端执行 → 按模式回调。
//!
//! step 1:骨架 + Mock 后端(可编译、可跑、单测绿)。step 2 接 claude/codex/opencode + git 快照。
//!
//! 环境变量:
//!   SHEPHERD_BASE(默认 http://127.0.0.1:9180)、MS_ADMIN_USER/MS_ADMIN_PASSWORD、
//!   SHEPHERD_CAPS(默认 CLAUDE_CODE)、RUNTIME_NAME、AGENT_MOCK(=1 用 mock 后端)。

// step 1 骨架:stream-json 解析器、git 相关字段等已实现并单测,但要到 step 2
// (claude/codex/opencode 真实后端 + git 快照)才被主流程调用。
#![allow(dead_code)]

mod backend;
mod client;
mod events;
mod models;

use std::sync::Arc;
use std::time::Duration;

use backend::{CliAgentBackend, MockBackend};
use client::{HttpSink, ServerClient};
use events::NoopSink;
use models::WorkSpec;

struct Config {
    base: String,
    user: String,
    pass: String,
    caps: Vec<String>,
    name: String,
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
        }
    }
}

/// 选择后端。step 1 恒为 Mock;step 2 按 executor 返回 claude/codex/opencode。
fn backend_for(_executor: &str) -> Arc<dyn CliAgentBackend> {
    Arc::new(MockBackend::default())
}

async fn handle(client: &ServerClient, spec: &WorkSpec) {
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
                // step 1:无 git 快照,占位交付物;step 2 换成真实 commit。
                let summary: String = output.chars().take(700).collect();
                client
                    .complete(
                        &spec.attempt_id,
                        "DIFF",
                        &format!("runtime://{}", spec.attempt_id),
                        &summary,
                    )
                    .await;
                tracing::info!(attempt = %spec.attempt_id, "交付完成(step1 占位)");
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
            Ok(Some(spec)) => handle(&client, &spec).await,
            Ok(None) => {} // 204:无活,继续长轮询
            Err(e) => {
                tracing::warn!("认领出错: {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}
