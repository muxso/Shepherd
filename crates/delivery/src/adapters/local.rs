use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::domain::{Deliverable, DeliverableKind, EventKind, ExecutorKind, NewExecutionEvent};
use crate::ports::{AgentExecutor, DispatchOutcome, EventSink, ExecError, WorkSpec};

#[derive(Clone)]
pub struct LocalCommandAgentExecutor {
    claude_code: Vec<String>,
    codex: Vec<String>,
    callback: Option<(String, String)>,
}

impl LocalCommandAgentExecutor {
    pub fn new(claude_code: Vec<String>, codex: Vec<String>) -> Self {
        Self { claude_code, codex, callback: None }
    }

    pub fn with_defaults() -> Self {
        Self {
            claude_code: vec!["claude".into(), "-p".into()],
            codex: vec!["codex".into(), "exec".into()],
            callback: None,
        }
    }

    pub fn with_async_callback(mut self, base: String, token: String) -> Self {
        self.callback = Some((base, token));
        self
    }

    fn argv(&self, kind: ExecutorKind) -> &[String] {
        match kind {
            ExecutorKind::ClaudeCode => &self.claude_code,
            ExecutorKind::Codex => &self.codex,
            // The local path only configures two argvs; OpenCode/CodeBuddy deliberately
            // fall back to the claude argv (real routing lives in crates/agent-runtime).
            ExecutorKind::OpenCode | ExecutorKind::CodeBuddy => &self.claude_code,
        }
    }
}

fn spec_to_prompt(spec: &WorkSpec) -> String {
    let mut p = String::new();
    if let Some(instr) = &spec.instructions {
        p.push_str(&format!("# Behavior (skills)\n{instr}\n\n"));
    }
    p.push_str(&format!("# Task: {}\n\n{}\n", spec.title, spec.description));
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

enum Line {
    Event(NewExecutionEvent),
    Result { reference: String, summary: String },
    Plain(String),
}

fn classify(line: &str) -> Line {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
        if let Some(kind) = v.get("event").and_then(|k| k.as_str()) {
            let message = v.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
            let detail = v.get("detail").and_then(|d| d.as_str());
            let kind = EventKind::parse(kind).unwrap_or(EventKind::Log);
            if let Ok(ev) = NewExecutionEvent::new(kind, &message, detail) {
                return Line::Event(ev);
            }
        }
        if v.get("reference").is_some() || v.get("summary").is_some() {
            return Line::Result {
                reference: v.get("reference").and_then(|r| r.as_str()).unwrap_or("").to_string(),
                summary: v.get("summary").and_then(|s| s.as_str()).unwrap_or("").to_string(),
            };
        }
    }
    Line::Plain(line.to_string())
}

#[async_trait]
impl AgentExecutor for LocalCommandAgentExecutor {
    async fn dispatch(
        &self,
        spec: &WorkSpec,
        sink: &dyn EventSink,
    ) -> Result<DispatchOutcome, ExecError> {
        let argv = self.argv(spec.executor);
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| ExecError::Backend("empty executor command".into()))?;

        if let Some((base, token)) = &self.callback {
            let mut child = Command::new(program)
                .args(args)
                .env("SHEPHERD_ATTEMPT_ID", &spec.attempt_id)
                .env("SHEPHERD_CALLBACK_URL", base)
                .env("SHEPHERD_CALLBACK_TOKEN", token)
                .env("SHEPHERD_EXECUTOR", spec.executor.as_str())
                .env("SHEPHERD_TASK_ID", &spec.task_id)
                .env("SHEPHERD_DECOMP_ID", &spec.decomposition_id)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| ExecError::Backend(format!("spawn {program}: {e}")))?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(spec_to_prompt(spec).as_bytes())
                    .await
                    .map_err(|e| ExecError::Backend(e.to_string()))?;
            }
            // The background wait() is required: an unreaped child becomes a zombie.
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            return Ok(DispatchOutcome::Accepted { run_id: spec.attempt_id.clone() });
        }

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
            // stdin drops here, sending EOF to the child (line reads would block forever otherwise)
        }

        let stdout = child.stdout.take().ok_or_else(|| ExecError::Backend("no stdout".into()))?;
        let mut lines = BufReader::new(stdout).lines();
        let mut result: Option<(String, String)> = None;
        while let Some(line) =
            lines.next_line().await.map_err(|e| ExecError::Backend(e.to_string()))?
        {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match classify(line) {
                Line::Event(ev) => sink.emit(ev).await,
                Line::Result { reference, summary } => result = Some((reference, summary)),
                Line::Plain(text) => {
                    if let Ok(ev) = NewExecutionEvent::new(EventKind::Log, &text, None) {
                        sink.emit(ev).await;
                    }
                }
            }
        }

        let status = child.wait().await.map_err(|e| ExecError::Backend(e.to_string()))?;
        if !status.success() {
            let mut err = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut err).await;
            }
            return Err(ExecError::Backend(format!("executor exited {}: {}", status, err.trim())));
        }

        let (reference, summary) =
            result.unwrap_or_else(|| (format!("local://{}", spec.task_id), String::new()));
        let reference =
            if reference.is_empty() { format!("local://{}", spec.task_id) } else { reference };
        Ok(DispatchOutcome::Completed {
            deliverable: Deliverable { kind: DeliverableKind::Diff, reference, summary },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::EventKind;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<(EventKind, String)>>,
    }
    #[async_trait]
    impl EventSink for RecordingSink {
        async fn emit(&self, e: NewExecutionEvent) {
            self.events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((e.kind, e.message));
        }
    }

    fn spec(kind: ExecutorKind) -> WorkSpec {
        WorkSpec {
            attempt_id: "a1".into(),
            decomposition_id: "d1".into(),
            task_id: "t1".into(),
            title: "build".into(),
            description: "do it".into(),
            acceptance_criteria: vec!["c1".into()],
            executor: kind,
            context: None,
            instructions: None,
            target_runtime: None,
        }
    }

    #[tokio::test]
    async fn streams_events_then_completes_with_result() {
        let script = r#"cat >/dev/null; printf '{"event":"DECISION","message":"用 argon2"}\n{"event":"FILE_CHANGE","message":"edit auth.rs"}\n{"reference":"branch:x","summary":"done"}\n'"#;
        let exec = LocalCommandAgentExecutor::new(
            vec!["/bin/sh".into(), "-c".into(), script.into()],
            vec!["/bin/sh".into(), "-c".into(), "true".into()],
        );
        let sink = RecordingSink::default();
        let out = exec.dispatch(&spec(ExecutorKind::ClaudeCode), &sink).await.expect("dispatch");
        match out {
            DispatchOutcome::Completed { deliverable } => {
                assert_eq!(deliverable.reference, "branch:x");
                assert_eq!(deliverable.summary, "done");
            }
            other => panic!("expected Completed, got {other:?}"),
        }
        let events = sink.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], (EventKind::Decision, "用 argon2".to_string()));
        assert_eq!(events[1].0, EventKind::FileChange);
    }

    #[tokio::test]
    async fn plain_lines_become_log_events() {
        let exec = LocalCommandAgentExecutor::new(
            vec!["/bin/sh".into(), "-c".into(), "cat >/dev/null; printf 'just text\\n'".into()],
            vec![],
        );
        let sink = RecordingSink::default();
        let out = exec.dispatch(&spec(ExecutorKind::ClaudeCode), &sink).await.expect("dispatch");
        match out {
            DispatchOutcome::Completed { deliverable } => {
                assert_eq!(deliverable.reference, "local://t1")
            }
            other => panic!("expected Completed, got {other:?}"),
        }
        let events = sink.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], (EventKind::Log, "just text".to_string()));
    }

    #[tokio::test]
    async fn nonzero_exit_is_backend_error() {
        let exec = LocalCommandAgentExecutor::new(
            vec!["/bin/sh".into(), "-c".into(), "echo boom >&2; exit 3".into()],
            vec![],
        );
        let sink = RecordingSink::default();
        assert!(matches!(
            exec.dispatch(&spec(ExecutorKind::ClaudeCode), &sink).await,
            Err(ExecError::Backend(_))
        ));
    }
}
