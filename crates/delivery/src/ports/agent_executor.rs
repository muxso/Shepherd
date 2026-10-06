use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{Deliverable, ExecutorKind};
use crate::ports::EventSink;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExecError {
    #[error("executor backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkSpec {
    pub attempt_id: String,
    pub decomposition_id: String,
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
    pub executor: ExecutorKind,
    pub context: Option<String>,
    pub instructions: Option<String>,
    /// Targeted dispatch: the name of a registered runtime (stable across reconnects,
    /// unlike the id which changes on every register). None = any runtime with the
    /// capability may claim.
    pub target_runtime: Option<String>,
}

impl WorkSpec {
    /// Canonical prompt rendering for executor CLIs. `agent-runtime` keeps a
    /// standalone mirror of this function (it deliberately avoids depending on
    /// this crate); the two must stay in sync.
    pub fn to_prompt(&self) -> String {
        let mut p = String::new();
        if let Some(instr) = &self.instructions {
            p.push_str(&format!("# Behavior (skills)\n{instr}\n\n"));
        }
        p.push_str(&format!("# Task: {}\n\n{}\n", self.title, self.description));
        if !self.acceptance_criteria.is_empty() {
            p.push_str("\nAcceptance criteria:\n");
            for c in &self.acceptance_criteria {
                p.push_str(&format!("- {c}\n"));
            }
        }
        if let Some(ctx) = &self.context {
            if ctx != "design" {
                p.push_str(&format!("\nContext: {ctx}\n"));
            }
        }
        p
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    Accepted { run_id: String },
    Completed { deliverable: Deliverable },
}

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn dispatch(
        &self,
        spec: &WorkSpec,
        sink: &dyn EventSink,
    ) -> Result<DispatchOutcome, ExecError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> WorkSpec {
        WorkSpec {
            attempt_id: "a".into(),
            decomposition_id: "d".into(),
            task_id: "t".into(),
            title: "login".into(),
            description: "do it".into(),
            acceptance_criteria: vec!["c1".into()],
            executor: ExecutorKind::ClaudeCode,
            context: Some("design".into()),
            instructions: Some("be careful".into()),
            target_runtime: None,
        }
    }

    #[test]
    fn prompt_renders_instructions_task_criteria_and_skips_design_context() {
        let p = spec().to_prompt();
        assert!(p.contains("# Behavior (skills)\nbe careful"));
        assert!(p.contains("# Task: login"));
        assert!(p.contains("- c1"));
        assert!(!p.contains("Context: design"));
    }
}
