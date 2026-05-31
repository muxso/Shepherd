//! 本地子进程执行者(feature = "exec-local"):spawn `claude`/`codex` headless,**同步**跑完。
//!
//! 约定:把任务规格作为提示写入子进程 stdin;子进程(或其 wrapper)在 stdout 输出
//! `{"reference": "...", "summary": "..."}` 表示交付物;非零退出 → `ExecError`。
//! 同步完成 → `DispatchOutcome::Completed`(对齐 api-test 原生 runner 的语义)。

use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::domain::{Deliverable, DeliverableKind, ExecutorKind};
use crate::ports::{AgentExecutor, DispatchOutcome, ExecError, WorkSpec};

/// 按执行者种类路由到不同的 argv(程序 + 参数)。
#[derive(Clone)]
pub struct LocalCommandAgentExecutor {
    claude_code: Vec<String>,
    codex: Vec<String>,
}

impl LocalCommandAgentExecutor {
    pub fn new(claude_code: Vec<String>, codex: Vec<String>) -> Self {
        Self { claude_code, codex }
    }

    /// 常见默认:`claude -p`(headless print)与 `codex exec`。
    pub fn with_defaults() -> Self {
        Self {
            claude_code: vec!["claude".into(), "-p".into()],
            codex: vec!["codex".into(), "exec".into()],
        }
    }

    fn argv(&self, kind: ExecutorKind) -> &[String] {
        match kind {
            ExecutorKind::ClaudeCode => &self.claude_code,
            ExecutorKind::Codex => &self.codex,
        }
    }
}

fn spec_to_prompt(spec: &WorkSpec) -> String {
    let mut p = format!("# Task: {}\n\n{}\n", spec.title, spec.description);
    if !spec.acceptance_criteria.is_empty() {
        p.push_str("\nAcceptance criteria:\n");
        for c in &spec.acceptance_criteria {
            p.push_str(&format!("- {c}\n"));
        }
    }
    if let Some(ctx) = &spec.context {
        p.push_str(&format!("\nContext: {ctx}\n"));
    }
    p
}

fn parse_result(stdout: &str, task_id: &str) -> (String, String) {
    #[derive(serde::Deserialize)]
    struct R {
        #[serde(default)]
        reference: Option<String>,
        #[serde(default)]
        summary: Option<String>,
    }
    if let Ok(r) = serde_json::from_str::<R>(stdout.trim()) {
        return (
            r.reference.unwrap_or_else(|| format!("local://{task_id}")),
            r.summary.unwrap_or_default(),
        );
    }
    (format!("local://{task_id}"), stdout.trim().to_string())
}

#[async_trait]
impl AgentExecutor for LocalCommandAgentExecutor {
    async fn dispatch(&self, spec: &WorkSpec) -> Result<DispatchOutcome, ExecError> {
        let argv = self.argv(spec.executor);
        let (program, args) =
            argv.split_first().ok_or_else(|| ExecError::Backend("empty executor command".into()))?;

        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ExecError::Backend(format!("spawn {program}: {e}")))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(spec_to_prompt(spec).as_bytes())
                .await
                .map_err(|e| ExecError::Backend(e.to_string()))?;
            // stdin 在此 drop → 向子进程发送 EOF
        }

        let out = child.wait_with_output().await.map_err(|e| ExecError::Backend(e.to_string()))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(ExecError::Backend(format!(
                "executor exited {}: {}",
                out.status,
                err.trim()
            )));
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let (reference, summary) = parse_result(&stdout, &spec.task_id);
        Ok(DispatchOutcome::Completed {
            deliverable: Deliverable { kind: DeliverableKind::Diff, reference, summary },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: ExecutorKind) -> WorkSpec {
        WorkSpec {
            decomposition_id: "d1".into(),
            task_id: "t1".into(),
            title: "build".into(),
            description: "do it".into(),
            acceptance_criteria: vec!["c1".into()],
            executor: kind,
            context: None,
        }
    }

    #[tokio::test]
    async fn completes_parsing_json_stdout() {
        // 子进程吞掉 stdin 后输出 JSON 交付物
        let exec = LocalCommandAgentExecutor::new(
            vec![
                "/bin/sh".into(),
                "-c".into(),
                r#"cat >/dev/null; printf '{"reference":"branch:test","summary":"ok"}'"#.into(),
            ],
            vec!["/bin/sh".into(), "-c".into(), "true".into()],
        );
        match exec.dispatch(&spec(ExecutorKind::ClaudeCode)).await.expect("dispatch") {
            DispatchOutcome::Completed { deliverable } => {
                assert_eq!(deliverable.reference, "branch:test");
                assert_eq!(deliverable.summary, "ok");
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn nonzero_exit_is_backend_error() {
        let exec = LocalCommandAgentExecutor::new(
            vec!["/bin/sh".into(), "-c".into(), "echo boom >&2; exit 3".into()],
            vec![],
        );
        assert!(matches!(
            exec.dispatch(&spec(ExecutorKind::ClaudeCode)).await,
            Err(ExecError::Backend(_))
        ));
    }

    #[tokio::test]
    async fn plain_stdout_becomes_summary() {
        let exec = LocalCommandAgentExecutor::new(
            vec!["/bin/sh".into(), "-c".into(), "cat >/dev/null; printf 'just text'".into()],
            vec![],
        );
        match exec.dispatch(&spec(ExecutorKind::ClaudeCode)).await.expect("dispatch") {
            DispatchOutcome::Completed { deliverable } => {
                assert_eq!(deliverable.reference, "local://t1");
                assert_eq!(deliverable.summary, "just text");
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }
}
