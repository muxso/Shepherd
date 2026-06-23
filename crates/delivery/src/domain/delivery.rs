//! 交付领域模型:执行者种类、交付物、交付尝试状态机。

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeliveryError {
    #[error("transition not allowed: {from} -> {to}")]
    TransitionNotAllowed { from: &'static str, to: &'static str },
}

/// 执行者种类(决定路由到哪种 agent)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorKind {
    ClaudeCode,
    Codex,
}

impl ExecutorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "CLAUDE_CODE",
            Self::Codex => "CODEX",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "CLAUDE_CODE" => Some(Self::ClaudeCode),
            "CODEX" => Some(Self::Codex),
            _ => None,
        }
    }
}

/// 交付物的形态。
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

/// 执行者产出的交付物:形态 + 引用(URL/分支/补丁)+ 摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deliverable {
    pub kind: DeliverableKind,
    pub reference: String,
    pub summary: String,
}

/// 交付尝试状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptStatus {
    /// 已派发给执行者,尚未确认开跑。
    Dispatched,
    /// 执行者已接单、运行中(异步)。
    Running,
    /// 已产出交付物(终态成功)。
    Delivered,
    /// 执行/交付失败(终态失败)。
    Failed,
    /// 用户主动停止(终态中止)。
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

    /// 是否终态(不再流转)。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Delivered | Self::Failed | Self::Stopped)
    }

    pub fn can_transition_to(self, to: AttemptStatus) -> bool {
        use AttemptStatus::*;
        matches!(
            (self, to),
            (Dispatched, Running)
                | (Dispatched, Delivered) // 同步执行器一步到位
                | (Dispatched, Failed)
                | (Dispatched, Stopped) // 派发后未开跑即被停
                | (Running, Delivered)
                | (Running, Failed)
                | (Running, Stopped) // 运行中被用户停止
        )
    }
}

/// 一次把某任务交给某执行者的交付尝试。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryAttempt {
    pub id: String,
    pub decomposition_id: String,
    pub task_id: String,
    pub executor: ExecutorKind,
    pub status: AttemptStatus,
    /// 异步执行器返回的运行 id(用于回调关联)。
    pub run_id: Option<String>,
    pub deliverable: Option<Deliverable>,
    pub error: Option<String>,
}

impl DeliveryAttempt {
    /// 新建一次尝试,初始 Dispatched。
    pub fn dispatched(
        id: &str,
        decomposition_id: &str,
        task_id: &str,
        executor: ExecutorKind,
    ) -> Self {
        Self {
            id: id.to_string(),
            decomposition_id: decomposition_id.to_string(),
            task_id: task_id.to_string(),
            executor,
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

    /// 异步执行者已接单开跑:Dispatched→Running,记录 run_id。
    pub fn start_running(&mut self, run_id: &str) -> Result<(), DeliveryError> {
        self.ensure(AttemptStatus::Running)?;
        self.status = AttemptStatus::Running;
        self.run_id = Some(run_id.to_string());
        Ok(())
    }

    /// 交付完成:Dispatched/Running→Delivered,带上交付物。
    pub fn deliver(&mut self, deliverable: Deliverable) -> Result<(), DeliveryError> {
        self.ensure(AttemptStatus::Delivered)?;
        self.status = AttemptStatus::Delivered;
        self.deliverable = Some(deliverable);
        Ok(())
    }

    /// 失败:Dispatched/Running→Failed,记录原因。
    pub fn fail(&mut self, error: &str) -> Result<(), DeliveryError> {
        self.ensure(AttemptStatus::Failed)?;
        self.status = AttemptStatus::Failed;
        self.error = Some(error.to_string());
        Ok(())
    }

    /// 用户主动停止:Dispatched/Running→Stopped,记录原因(终态中止)。
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
        DeliveryAttempt::dispatched("a1", "d1", "t1", ExecutorKind::ClaudeCode)
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
        for e in [ExecutorKind::ClaudeCode, ExecutorKind::Codex] {
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

        // 终态不可再停
        assert!(a.stop("again").is_err());
    }
}
