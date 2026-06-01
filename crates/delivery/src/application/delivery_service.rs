//! 用例:交付编排 —— 派发任务给执行者并按 `DispatchOutcome` 推进尝试;异步回调收尾。

use std::sync::Arc;

use crate::domain::{
    AttemptStatus, Deliverable, DeliverableKind, DeliveryAttempt, DeliveryError, EventKind,
    ExecutionEvent, ExecutorKind, NewExecutionEvent,
};
use async_trait::async_trait;

use crate::ports::{
    AgentExecutor, DeliveryObserver, DeliveryRepository, DispatchOutcome, EventSink, ExecError,
    RepoError, WorkSpec,
};

/// 把执行者运行中 emit 的事件落到当前交付尝试(尽力而为)。
struct RepoEventSink {
    repo: Arc<dyn DeliveryRepository>,
    attempt_id: String,
}

#[async_trait]
impl EventSink for RepoEventSink {
    async fn emit(&self, event: crate::domain::NewExecutionEvent) {
        let _ = self.repo.append_event(&self.attempt_id, &event).await;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryCmdError {
    /// 尝试不存在 → 404。
    NotFound,
    /// 入参非法(未知执行者/交付物形态、空引用)→ 400。
    Validation(String),
    /// 非法状态流转(对终态回调等)→ 409。
    Conflict(DeliveryError),
    /// 存储错误 → 500。
    Repo(RepoError),
}

impl From<RepoError> for DeliveryCmdError {
    fn from(e: RepoError) -> Self {
        Self::Repo(e)
    }
}

impl From<DeliveryError> for DeliveryCmdError {
    fn from(e: DeliveryError) -> Self {
        Self::Conflict(e)
    }
}

#[derive(Clone)]
pub struct DeliveryService {
    repo: Arc<dyn DeliveryRepository>,
    executor: Arc<dyn AgentExecutor>,
    /// 可选观察者:尝试落终态后通知(组装根桥接到 orchestrator)。
    observer: Option<Arc<dyn DeliveryObserver>>,
}

impl DeliveryService {
    pub fn new(repo: Arc<dyn DeliveryRepository>, executor: Arc<dyn AgentExecutor>) -> Self {
        Self { repo, executor, observer: None }
    }

    /// 挂上观察者(终态通知)。
    pub fn with_observer(mut self, observer: Arc<dyn DeliveryObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    /// 尝试进入 Running/Delivered/Failed 时通知观察者(尽力而为)。
    async fn notify_progress(&self, attempt: &DeliveryAttempt) {
        if matches!(
            attempt.status,
            AttemptStatus::Running | AttemptStatus::Delivered | AttemptStatus::Failed
        ) {
            if let Some(o) = &self.observer {
                o.on_progress(attempt).await;
            }
        }
    }

    /// 派发一个任务给执行者:建尝试 → 调执行者 → 按结果推进。
    /// 执行者后端错误**不丢尝试**,记为 Failed 后照常返回(对齐 api-test 的"别卡在中间态")。
    #[allow(clippy::too_many_arguments)]
    pub async fn dispatch(
        &self,
        decomposition_id: &str,
        task_id: &str,
        title: &str,
        description: &str,
        acceptance_criteria: &[String],
        executor: &str,
        context: Option<String>,
        instructions: Option<String>,
    ) -> Result<DeliveryAttempt, DeliveryCmdError> {
        if decomposition_id.trim().is_empty() || task_id.trim().is_empty() {
            return Err(DeliveryCmdError::Validation("decompositionId/taskId required".into()));
        }
        if title.trim().is_empty() {
            return Err(DeliveryCmdError::Validation("title required".into()));
        }
        let kind = ExecutorKind::parse(executor)
            .ok_or_else(|| DeliveryCmdError::Validation(format!("unknown executor: {executor}")))?;

        let mut attempt = self.repo.create(decomposition_id, task_id, kind).await?;
        let spec = WorkSpec {
            decomposition_id: decomposition_id.to_string(),
            task_id: task_id.to_string(),
            title: title.trim().to_string(),
            description: description.trim().to_string(),
            acceptance_criteria: acceptance_criteria.to_vec(),
            executor: kind,
            context,
            instructions,
        };

        // 执行者运行中 emit 的事件落到本次尝试。
        let sink = RepoEventSink { repo: self.repo.clone(), attempt_id: attempt.id.clone() };
        match self.executor.dispatch(&spec, &sink).await {
            Ok(DispatchOutcome::Accepted { run_id }) => attempt.start_running(&run_id)?,
            Ok(DispatchOutcome::Completed { deliverable }) => attempt.deliver(deliverable)?,
            Err(ExecError::Backend(msg)) => attempt.fail(&msg)?,
        }
        self.repo.save(&attempt).await?;
        self.notify_progress(&attempt).await;
        Ok(attempt)
    }

    pub async fn get(&self, id: &str) -> Result<DeliveryAttempt, DeliveryCmdError> {
        self.repo.get(id).await?.ok_or(DeliveryCmdError::NotFound)
    }

    pub async fn list_by_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<DeliveryAttempt>, DeliveryCmdError> {
        Ok(self.repo.list_by_task(decomposition_id, task_id).await?)
    }

    /// 异步回调:执行者已接单开跑。
    pub async fn report_running(
        &self,
        id: &str,
        run_id: &str,
    ) -> Result<DeliveryAttempt, DeliveryCmdError> {
        let mut a = self.get(id).await?;
        a.start_running(run_id)?;
        self.repo.save(&a).await?;
        self.notify_progress(&a).await;
        Ok(a)
    }

    /// 异步回调:执行者交付完成。
    pub async fn complete(
        &self,
        id: &str,
        kind: &str,
        reference: &str,
        summary: &str,
    ) -> Result<DeliveryAttempt, DeliveryCmdError> {
        let kind = DeliverableKind::parse(kind)
            .ok_or_else(|| DeliveryCmdError::Validation(format!("unknown deliverable kind: {kind}")))?;
        let mut a = self.get(id).await?;
        a.deliver(Deliverable {
            kind,
            reference: reference.to_string(),
            summary: summary.to_string(),
        })?;
        self.repo.save(&a).await?;
        self.notify_progress(&a).await;
        Ok(a)
    }

    /// 记录一条执行事件(AI 执行者上报决策/文件变更/测试结果;审计轨迹)。
    pub async fn record_event(
        &self,
        attempt_id: &str,
        kind: &str,
        message: &str,
        detail: Option<&str>,
    ) -> Result<ExecutionEvent, DeliveryCmdError> {
        // 尝试须存在
        self.get(attempt_id).await?;
        let kind = EventKind::parse(kind)
            .ok_or_else(|| DeliveryCmdError::Validation(format!("unknown event kind: {kind}")))?;
        let new = NewExecutionEvent::new(kind, message, detail)
            .map_err(|e| DeliveryCmdError::Validation(e.to_string()))?;
        Ok(self.repo.append_event(attempt_id, &new).await?)
    }

    /// 某尝试的执行事件(审计读取)。
    pub async fn events(&self, attempt_id: &str) -> Result<Vec<ExecutionEvent>, DeliveryCmdError> {
        self.get(attempt_id).await?;
        Ok(self.repo.list_events(attempt_id).await?)
    }

    /// 异步回调:执行者失败。
    pub async fn fail(&self, id: &str, error: &str) -> Result<DeliveryAttempt, DeliveryCmdError> {
        let mut a = self.get(id).await?;
        a.fail(error)?;
        self.repo.save(&a).await?;
        self.notify_progress(&a).await;
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{InMemoryDeliveryRepository, StubAgentExecutor, StubBehavior};
    use crate::domain::AttemptStatus;

    fn svc(behavior: StubBehavior) -> DeliveryService {
        DeliveryService::new(
            Arc::new(InMemoryDeliveryRepository::new()),
            Arc::new(StubAgentExecutor::new(behavior)),
        )
    }

    fn deliverable() -> Deliverable {
        Deliverable {
            kind: DeliverableKind::PullRequest,
            reference: "https://example/pr/1".into(),
            summary: "done".into(),
        }
    }

    #[tokio::test]
    async fn sync_executor_completes_to_delivered() {
        let s = svc(StubBehavior::Complete { deliverable: deliverable() });
        let a = s
            .dispatch("d1", "t1", "build login", "do it", &["c1".into()], "CLAUDE_CODE", None, None)
            .await
            .expect("dispatch");
        assert_eq!(a.status, AttemptStatus::Delivered);
        assert_eq!(a.deliverable.as_ref().expect("d").kind, DeliverableKind::PullRequest);
    }

    #[tokio::test]
    async fn async_executor_goes_running_then_callback_completes() {
        let s = svc(StubBehavior::Accept { run_id: "run-1".into() });
        let a = s
            .dispatch("d1", "t1", "build", "", &[], "CODEX", None, None)
            .await
            .expect("dispatch");
        assert_eq!(a.status, AttemptStatus::Running);
        assert_eq!(a.run_id.as_deref(), Some("run-1"));

        // 异步回调交付
        let done = s.complete(&a.id, "DIFF", "branch:x", "ok").await.expect("complete");
        assert_eq!(done.status, AttemptStatus::Delivered);
        assert_eq!(s.get(&a.id).await.expect("get").status, AttemptStatus::Delivered);
    }

    #[tokio::test]
    async fn executor_backend_error_records_failed_not_lost() {
        let s = svc(StubBehavior::Error { message: "spawn failed".into() });
        let a = s.dispatch("d1", "t1", "build", "", &[], "CLAUDE_CODE", None, None).await.expect("dispatch");
        assert_eq!(a.status, AttemptStatus::Failed);
        assert_eq!(a.error.as_deref(), Some("spawn failed"));
    }

    #[tokio::test]
    async fn unknown_executor_is_validation() {
        let s = svc(StubBehavior::Accept { run_id: "r".into() });
        assert!(matches!(
            s.dispatch("d1", "t1", "x", "", &[], "GPT5", None, None).await.unwrap_err(),
            DeliveryCmdError::Validation(_)
        ));
    }

    #[tokio::test]
    async fn callback_on_terminal_is_conflict() {
        let s = svc(StubBehavior::Complete { deliverable: deliverable() });
        let a = s.dispatch("d1", "t1", "x", "", &[], "CODEX", None, None).await.expect("dispatch");
        // 已 Delivered,再 report_running 应冲突
        assert!(matches!(
            s.report_running(&a.id, "r").await.unwrap_err(),
            DeliveryCmdError::Conflict(_)
        ));
    }

    #[tokio::test]
    async fn observer_notified_on_terminal_settlement() {
        use crate::ports::DeliveryObserver;
        use async_trait::async_trait;
        use std::sync::Mutex;

        #[derive(Default)]
        struct Spy {
            settled: Mutex<Vec<(String, String)>>, // (task_id, status)
        }
        #[async_trait]
        impl DeliveryObserver for Spy {
            async fn on_progress(&self, a: &DeliveryAttempt) {
                self.settled.lock().unwrap().push((a.task_id.clone(), a.status.as_str().to_string()));
            }
        }

        let spy = Arc::new(Spy::default());
        let svc = DeliveryService::new(
            Arc::new(InMemoryDeliveryRepository::new()),
            Arc::new(StubAgentExecutor::new(StubBehavior::Complete { deliverable: deliverable() })),
        )
        .with_observer(spy.clone());

        // 同步完成 → Delivered 终态 → 观察者被通知一次
        svc.dispatch("d1", "t1", "x", "", &[], "CLAUDE_CODE", None, None).await.expect("dispatch");
        assert_eq!(spy.settled.lock().unwrap().as_slice(), &[("t1".into(), "DELIVERED".into())]);
    }

    #[tokio::test]
    async fn observer_notified_on_running_progress() {
        use crate::ports::DeliveryObserver;
        use async_trait::async_trait;
        use std::sync::Mutex;

        #[derive(Default)]
        struct Spy {
            n: Mutex<usize>,
        }
        #[async_trait]
        impl DeliveryObserver for Spy {
            async fn on_progress(&self, _a: &DeliveryAttempt) {
                *self.n.lock().unwrap() += 1;
            }
        }

        let spy = Arc::new(Spy::default());
        let svc = DeliveryService::new(
            Arc::new(InMemoryDeliveryRepository::new()),
            Arc::new(StubAgentExecutor::new(StubBehavior::Accept { run_id: "r".into() })),
        )
        .with_observer(spy.clone());

        // 异步接单 → Running(进度推进)→ 通知一次(供编排器驱动任务到 Running)
        svc.dispatch("d1", "t1", "x", "", &[], "CODEX", None, None).await.expect("dispatch");
        assert_eq!(*spy.n.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn executor_emitted_events_are_recorded_automatically() {
        use crate::adapters::EchoAgentExecutor;
        // Echo 执行者运行中 emit 一条 LOG 事件 → 应自动落到该尝试,无需手动 record_event
        let s = DeliveryService::new(
            Arc::new(InMemoryDeliveryRepository::new()),
            Arc::new(EchoAgentExecutor::new()),
        );
        let a = s.dispatch("d1", "t1", "实现登录", "", &[], "CLAUDE_CODE", None, None).await.expect("dispatch");
        let events = s.events(&a.id).await.expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::Log);
    }

    #[tokio::test]
    async fn records_and_lists_execution_events() {
        let s = svc(StubBehavior::Accept { run_id: "r".into() });
        let a = s.dispatch("d1", "t1", "build", "", &[], "CLAUDE_CODE", None, None).await.expect("dispatch");

        s.record_event(&a.id, "DECISION", "选用 argon2", Some("PHC 格式")).await.expect("ev1");
        s.record_event(&a.id, "FILE_CHANGE", "edit auth.rs", None).await.expect("ev2");
        s.record_event(&a.id, "TEST_RESULT", "cargo test 通过", None).await.expect("ev3");

        let events = s.events(&a.id).await.expect("events");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, EventKind::Decision);
        assert!(events[0].seq < events[1].seq && events[1].seq < events[2].seq); // 单调
        assert_eq!(events[1].message, "edit auth.rs");
    }

    #[tokio::test]
    async fn record_event_rejects_unknown_kind_and_missing_attempt() {
        let s = svc(StubBehavior::Accept { run_id: "r".into() });
        let a = s.dispatch("d1", "t1", "x", "", &[], "CODEX", None, None).await.expect("dispatch");
        assert!(matches!(
            s.record_event(&a.id, "WHAT", "m", None).await.unwrap_err(),
            DeliveryCmdError::Validation(_)
        ));
        assert_eq!(
            s.record_event("ghost", "LOG", "m", None).await.unwrap_err(),
            DeliveryCmdError::NotFound
        );
    }

    #[tokio::test]
    async fn list_by_task_and_not_found() {
        let s = svc(StubBehavior::Accept { run_id: "r".into() });
        s.dispatch("d1", "t1", "x", "", &[], "CODEX", None, None).await.expect("d");
        s.dispatch("d1", "t1", "y", "", &[], "CODEX", None, None).await.expect("d");
        assert_eq!(s.list_by_task("d1", "t1").await.expect("list").len(), 2);
        assert_eq!(s.get("ghost").await.unwrap_err(), DeliveryCmdError::NotFound);
    }
}
