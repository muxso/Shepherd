use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeliveryError {
    #[error("transition not allowed: {from} -> {to}")]
    TransitionNotAllowed { from: &'static str, to: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorKind {
    ClaudeCode,
    Codex,
    OpenCode,
    CodeBuddy,
}

impl ExecutorKind {
    /// 全部执行者类型的唯一权威清单;队列适配器与测试都从这里取,新增变体只改本文件。
    pub const ALL: [ExecutorKind; 4] =
        [Self::ClaudeCode, Self::Codex, Self::OpenCode, Self::CodeBuddy];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "CLAUDE_CODE",
            Self::Codex => "CODEX",
            Self::OpenCode => "OPENCODE",
            Self::CodeBuddy => "CODEBUDDY",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "CLAUDE_CODE" => Some(Self::ClaudeCode),
            "CODEX" => Some(Self::Codex),
            "OPENCODE" => Some(Self::OpenCode),
            "CODEBUDDY" => Some(Self::CodeBuddy),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliverableKind {
    Diff,
    PullRequest,
    Branch,
    Patch,
}

impl DeliverableKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Diff => "DIFF",
            Self::PullRequest => "PULL_REQUEST",
            Self::Branch => "BRANCH",
            Self::Patch => "PATCH",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "DIFF" => Some(Self::Diff),
            "PULL_REQUEST" => Some(Self::PullRequest),
            "BRANCH" => Some(Self::Branch),
            "PATCH" => Some(Self::Patch),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deliverable {
    pub kind: DeliverableKind,
    pub reference: String,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptStatus {
    Dispatched,
    Running,
    Delivered,
    Failed,
    Stopped,
}

impl AttemptStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dispatched => "DISPATCHED",
            Self::Running => "RUNNING",
            Self::Delivered => "DELIVERED",
            Self::Failed => "FAILED",
            Self::Stopped => "STOPPED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "DISPATCHED" => Some(Self::Dispatched),
            "RUNNING" => Some(Self::Running),
            "DELIVERED" => Some(Self::Delivered),
            "FAILED" => Some(Self::Failed),
            "STOPPED" => Some(Self::Stopped),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Delivered | Self::Failed | Self::Stopped)
    }

    pub fn can_transition_to(self, to: AttemptStatus) -> bool {
        use AttemptStatus::*;
        matches!(
            (self, to),
            (Dispatched, Running)
                | (Dispatched, Delivered)
                | (Dispatched, Failed)
                | (Dispatched, Stopped)
                | (Running, Delivered)
                | (Running, Failed)
                | (Running, Stopped)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryAttempt {
    pub id: String,
    pub decomposition_id: String,
    pub task_id: String,
    pub executor: ExecutorKind,
    /// 定向派发的目标 runtime name;None = 任意同能力 runtime。
    pub target_runtime: Option<String>,
    pub status: AttemptStatus,
    pub run_id: Option<String>,
    pub deliverable: Option<Deliverable>,
    pub error: Option<String>,
}

impl DeliveryAttempt {
    pub fn dispatched(
        id: &str,
        decomposition_id: &str,
        task_id: &str,
        executor: ExecutorKind,
        target_runtime: Option<&str>,
    ) -> Self {
        Self {
            id: id.to_string(),
            decomposition_id: decomposition_id.to_string(),
            task_id: task_id.to_string(),
            executor,
            target_runtime: target_runtime.map(|s| s.to_string()),
            status: AttemptStatus::Dispatched,
            run_id: None,
            deliverable: None,
            error: None,
        }
    }

    fn ensure(&self, to: AttemptStatus) -> Result<(), DeliveryError> {
        if self.status.can_transition_to(to) {
            Ok(())
        } else {
            Err(DeliveryError::TransitionNotAllowed { from: self.status.as_str(), to: to.as_str() })
        }
    }

    pub fn start_running(&mut self, run_id: &str) -> Result<(), DeliveryError> {
        self.ensure(AttemptStatus::Running)?;
        self.status = AttemptStatus::Running;
        self.run_id = Some(run_id.to_string());
        Ok(())
    }

    pub fn deliver(&mut self, deliverable: Deliverable) -> Result<(), DeliveryError> {
        self.ensure(AttemptStatus::Delivered)?;
        self.status = AttemptStatus::Delivered;
        self.deliverable = Some(deliverable);
        Ok(())
    }

    pub fn fail(&mut self, error: &str) -> Result<(), DeliveryError> {
        self.ensure(AttemptStatus::Failed)?;
        self.status = AttemptStatus::Failed;
        self.error = Some(error.to_string());
        Ok(())
    }

    pub fn stop(&mut self, reason: &str) -> Result<(), DeliveryError> {
        self.ensure(AttemptStatus::Stopped)?;
        self.status = AttemptStatus::Stopped;
        self.error = Some(reason.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deliverable() -> Deliverable {
        Deliverable {
            kind: DeliverableKind::Diff,
            reference: "branch:feat/login".into(),
            summary: "implemented login".into(),
        }
    }

    fn attempt() -> DeliveryAttempt {
        DeliveryAttempt::dispatched("a1", "d1", "t1", ExecutorKind::ClaudeCode, None)
    }

    #[test]
    fn new_attempt_is_dispatched() {
        let a = attempt();
        assert_eq!(a.status, AttemptStatus::Dispatched);
        assert!(a.run_id.is_none() && a.deliverable.is_none() && a.error.is_none());
    }

    #[test]
    fn async_path_dispatched_running_delivered() {
        let mut a = attempt();
        a.start_running("run-7").expect("running");
        assert_eq!(a.status, AttemptStatus::Running);
        assert_eq!(a.run_id.as_deref(), Some("run-7"));
        a.deliver(deliverable()).expect("deliver");
        assert_eq!(a.status, AttemptStatus::Delivered);
        assert_eq!(a.deliverable.as_ref().expect("d").kind, DeliverableKind::Diff);
    }

    #[test]
    fn sync_path_dispatched_straight_to_delivered() {
        let mut a = attempt();
        a.deliver(deliverable()).expect("deliver");
        assert_eq!(a.status, AttemptStatus::Delivered);
    }

    #[test]
    fn fail_from_dispatched_or_running() {
        let mut a = attempt();
        a.fail("boom").expect("fail");
        assert_eq!(a.status, AttemptStatus::Failed);
        assert_eq!(a.error.as_deref(), Some("boom"));

        let mut b = attempt();
        b.start_running("r").expect("run");
        assert!(b.fail("mid-run crash").is_ok());
    }

    #[test]
    fn terminal_states_reject_transitions() {
        let mut a = attempt();
        a.deliver(deliverable()).expect("deliver");
        assert_eq!(
            a.start_running("r").unwrap_err(),
            DeliveryError::TransitionNotAllowed { from: "DELIVERED", to: "RUNNING" }
        );
        assert!(a.fail("x").is_err());
    }

    #[test]
    fn enum_str_roundtrips() {
        for e in ExecutorKind::ALL {
            assert_eq!(ExecutorKind::parse(e.as_str()), Some(e));
        }
        assert_eq!(ExecutorKind::parse("X"), None);
        for k in [DeliverableKind::Diff, DeliverableKind::PullRequest, DeliverableKind::Branch, DeliverableKind::Patch] {
            assert_eq!(DeliverableKind::parse(k.as_str()), Some(k));
        }
        for s in [AttemptStatus::Dispatched, AttemptStatus::Running, AttemptStatus::Delivered, AttemptStatus::Failed, AttemptStatus::Stopped] {
            assert_eq!(AttemptStatus::parse(s.as_str()), Some(s));
        }
    }

    #[test]
    fn stop_from_dispatched_or_running_then_terminal() {
        let mut a = attempt();
        a.stop("用户停止").expect("stop");
        assert_eq!(a.status, AttemptStatus::Stopped);
        assert_eq!(a.error.as_deref(), Some("用户停止"));
        assert!(a.status.is_terminal());

        let mut b = attempt();
        b.start_running("r").expect("run");
        assert!(b.stop("中途停止").is_ok());

        assert!(a.stop("again").is_err());
    }
}
