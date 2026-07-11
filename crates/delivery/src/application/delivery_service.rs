use std::sync::Arc;

use crate::domain::{
    AttemptStatus, Deliverable, DeliverableKind, DeliveryAttempt, DeliveryError, EventKind,
    ExecutionEvent, ExecutorKind, NewExecutionEvent,
};
use async_trait::async_trait;

use crate::ports::{
    AgentExecutor, DeliveryObserver, DeliveryRepository, DispatchOutcome, EventSink, ExecError,
    RepoError, TaskListFilter, TaskPage, WorkQueue, WorkSpec,
};

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
    NotFound,
    Validation(String),
    Conflict(DeliveryError),
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
    observer: Option<Arc<dyn DeliveryObserver>>,
    queue: Option<Arc<dyn WorkQueue>>,
}

impl DeliveryService {
    pub fn new(repo: Arc<dyn DeliveryRepository>, executor: Arc<dyn AgentExecutor>) -> Self {
        Self { repo, executor, observer: None, queue: None }
    }

    pub fn with_observer(mut self, observer: Arc<dyn DeliveryObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    pub fn with_queue(mut self, queue: Arc<dyn WorkQueue>) -> Self {
        self.queue = Some(queue);
        self
    }

    // 终态才 ack:把消息移出 Redis Streams 的 PEL,免得被死 runtime 回收逻辑重投。
    async fn ack_if_terminal(&self, attempt: &DeliveryAttempt) {
        if attempt.status.is_terminal() {
            if let Some(q) = &self.queue {
                q.ack(&attempt.id).await;
            }
        }
    }

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

    // 执行者后端错误不向上传播,而是把尝试记为 Failed 后照常返回,避免卡在中间态。
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
        target_runtime: Option<String>,
    ) -> Result<DeliveryAttempt, DeliveryCmdError> {
        if decomposition_id.trim().is_empty() || task_id.trim().is_empty() {
            return Err(DeliveryCmdError::Validation("decompositionId/taskId required".into()));
        }
        if title.trim().is_empty() {
            return Err(DeliveryCmdError::Validation("title required".into()));
        }
        let kind = ExecutorKind::parse(executor)
            .ok_or_else(|| DeliveryCmdError::Validation(format!("unknown executor: {executor}")))?;
        // 空白当未定向;定向 name 原样透传,是否在线不在此校验(离线 runtime 回来后仍可认领)。
        let target_runtime = target_runtime.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

        let mut attempt =
            self.repo.create(decomposition_id, task_id, kind, target_runtime.as_deref()).await?;
        let spec = WorkSpec {
            attempt_id: attempt.id.clone(),
            decomposition_id: decomposition_id.to_string(),
            task_id: task_id.to_string(),
            title: title.trim().to_string(),
            description: description.trim().to_string(),
            acceptance_criteria: acceptance_criteria.to_vec(),
            executor: kind,
            context,
            instructions,
            target_runtime,
        };

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

    pub async fn complete(
        &self,
        id: &str,
        kind: &str,
        reference: &str,
        summary: &str,
    ) -> Result<DeliveryAttempt, DeliveryCmdError> {
        let kind = DeliverableKind::parse(kind).ok_or_else(|| {
            DeliveryCmdError::Validation(format!("unknown deliverable kind: {kind}"))
        })?;
        let mut a = self.get(id).await?;
        a.deliver(Deliverable {
            kind,
            reference: reference.to_string(),
            summary: summary.to_string(),
        })?;
        self.repo.save(&a).await?;
        self.ack_if_terminal(&a).await;
        self.notify_progress(&a).await;
        Ok(a)
    }

    pub async fn record_event(
        &self,
        attempt_id: &str,
        kind: &str,
        message: &str,
        detail: Option<&str>,
    ) -> Result<ExecutionEvent, DeliveryCmdError> {
        self.get(attempt_id).await?;
        let kind = EventKind::parse(kind)
            .ok_or_else(|| DeliveryCmdError::Validation(format!("unknown event kind: {kind}")))?;
        let new = NewExecutionEvent::new(kind, message, detail)
            .map_err(|e| DeliveryCmdError::Validation(e.to_string()))?;
        Ok(self.repo.append_event(attempt_id, &new).await?)
    }

    pub async fn events(&self, attempt_id: &str) -> Result<Vec<ExecutionEvent>, DeliveryCmdError> {
        self.get(attempt_id).await?;
        Ok(self.repo.list_events(attempt_id).await?)
    }

    pub async fn fail(&self, id: &str, error: &str) -> Result<DeliveryAttempt, DeliveryCmdError> {
        let mut a = self.get(id).await?;
        a.fail(error)?;
        self.repo.save(&a).await?;
        self.ack_if_terminal(&a).await;
        self.notify_progress(&a).await;
        Ok(a)
    }

    pub async fn list_tasks(&self, filter: &TaskListFilter) -> Result<TaskPage, DeliveryCmdError> {
        Ok(self.repo.list_tasks(filter).await?)
    }

    pub async fn stop(&self, id: &str, reason: &str) -> Result<DeliveryAttempt, DeliveryCmdError> {
        let mut a = self.get(id).await?;
        let reason = if reason.trim().is_empty() { "stopped by user" } else { reason.trim() };
        a.stop(reason)?;
        self.repo.save(&a).await?;
        self.ack_if_terminal(&a).await;
        Ok(a)
    }

    pub async fn delete(&self, id: &str) -> Result<(), DeliveryCmdError> {
        let a = self.get(id).await?;
        if !a.status.is_terminal() {
            return Err(DeliveryCmdError::Conflict(DeliveryError::TransitionNotAllowed {
                from: a.status.as_str(),
                to: "DELETED",
            }));
        }
        if !self.repo.delete(id).await? {
            return Err(DeliveryCmdError::NotFound);
        }
        Ok(())
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
            .dispatch(
                "d1",
                "t1",
                "build login",
                "do it",
                &["c1".into()],
                "CLAUDE_CODE",
                None,
                None,
                None,
            )
            .await
            .expect("dispatch");
        assert_eq!(a.status, AttemptStatus::Delivered);
        assert_eq!(a.deliverable.as_ref().expect("d").kind, DeliverableKind::PullRequest);
    }

    #[tokio::test]
    async fn async_executor_goes_running_then_callback_completes() {
        let s = svc(StubBehavior::Accept { run_id: "run-1".into() });
        let a = s
            .dispatch("d1", "t1", "build", "", &[], "CODEX", None, None, None)
            .await
            .expect("dispatch");
        assert_eq!(a.status, AttemptStatus::Running);
        assert_eq!(a.run_id.as_deref(), Some("run-1"));

        let done = s.complete(&a.id, "DIFF", "branch:x", "ok").await.expect("complete");
        assert_eq!(done.status, AttemptStatus::Delivered);
        assert_eq!(s.get(&a.id).await.expect("get").status, AttemptStatus::Delivered);
    }

    #[tokio::test]
    async fn executor_backend_error_records_failed_not_lost() {
        let s = svc(StubBehavior::Error { message: "spawn failed".into() });
        let a = s
            .dispatch("d1", "t1", "build", "", &[], "CLAUDE_CODE", None, None, None)
            .await
            .expect("dispatch");
        assert_eq!(a.status, AttemptStatus::Failed);
        assert_eq!(a.error.as_deref(), Some("spawn failed"));
    }

    #[tokio::test]
    async fn unknown_executor_is_validation() {
        let s = svc(StubBehavior::Accept { run_id: "r".into() });
        assert!(matches!(
            s.dispatch("d1", "t1", "x", "", &[], "GPT5", None, None, None).await.unwrap_err(),
            DeliveryCmdError::Validation(_)
        ));
    }

    #[tokio::test]
    async fn callback_on_terminal_is_conflict() {
        let s = svc(StubBehavior::Complete { deliverable: deliverable() });
        let a = s
            .dispatch("d1", "t1", "x", "", &[], "CODEX", None, None, None)
            .await
            .expect("dispatch");
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
            settled: Mutex<Vec<(String, String)>>,
        }
        #[async_trait]
        impl DeliveryObserver for Spy {
            async fn on_progress(&self, a: &DeliveryAttempt) {
                self.settled
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push((a.task_id.clone(), a.status.as_str().to_string()));
            }
        }

        let spy = Arc::new(Spy::default());
        let svc = DeliveryService::new(
            Arc::new(InMemoryDeliveryRepository::new()),
            Arc::new(StubAgentExecutor::new(StubBehavior::Complete { deliverable: deliverable() })),
        )
        .with_observer(spy.clone());

        svc.dispatch("d1", "t1", "x", "", &[], "CLAUDE_CODE", None, None, None)
            .await
            .expect("dispatch");
        assert_eq!(
            spy.settled.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_slice(),
            &[("t1".into(), "DELIVERED".into())]
        );
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
                *self.n.lock().unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
            }
        }

        let spy = Arc::new(Spy::default());
        let svc = DeliveryService::new(
            Arc::new(InMemoryDeliveryRepository::new()),
            Arc::new(StubAgentExecutor::new(StubBehavior::Accept { run_id: "r".into() })),
        )
        .with_observer(spy.clone());

        svc.dispatch("d1", "t1", "x", "", &[], "CODEX", None, None, None).await.expect("dispatch");
        assert_eq!(*spy.n.lock().unwrap_or_else(std::sync::PoisonError::into_inner), 1);
    }

    #[tokio::test]
    async fn executor_emitted_events_are_recorded_automatically() {
        use crate::adapters::EchoAgentExecutor;
        let s = DeliveryService::new(
            Arc::new(InMemoryDeliveryRepository::new()),
            Arc::new(EchoAgentExecutor::new()),
        );
        let a = s
            .dispatch("d1", "t1", "实现登录", "", &[], "CLAUDE_CODE", None, None, None)
            .await
            .expect("dispatch");
        let events = s.events(&a.id).await.expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::Log);
    }

    #[tokio::test]
    async fn records_and_lists_execution_events() {
        let s = svc(StubBehavior::Accept { run_id: "r".into() });
        let a = s
            .dispatch("d1", "t1", "build", "", &[], "CLAUDE_CODE", None, None, None)
            .await
            .expect("dispatch");

        s.record_event(&a.id, "DECISION", "选用 argon2", Some("PHC 格式")).await.expect("ev1");
        s.record_event(&a.id, "FILE_CHANGE", "edit auth.rs", None).await.expect("ev2");
        s.record_event(&a.id, "TEST_RESULT", "cargo test 通过", None).await.expect("ev3");

        let events = s.events(&a.id).await.expect("events");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, EventKind::Decision);
        assert!(events[0].seq < events[1].seq && events[1].seq < events[2].seq);
        assert_eq!(events[1].message, "edit auth.rs");
    }

    #[tokio::test]
    async fn record_event_rejects_unknown_kind_and_missing_attempt() {
        let s = svc(StubBehavior::Accept { run_id: "r".into() });
        let a = s
            .dispatch("d1", "t1", "x", "", &[], "CODEX", None, None, None)
            .await
            .expect("dispatch");
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
        s.dispatch("d1", "t1", "x", "", &[], "CODEX", None, None, None).await.expect("d");
        s.dispatch("d1", "t1", "y", "", &[], "CODEX", None, None, None).await.expect("d");
        assert_eq!(s.list_by_task("d1", "t1").await.expect("list").len(), 2);
        assert_eq!(s.get("ghost").await.unwrap_err(), DeliveryCmdError::NotFound);
    }

    #[tokio::test]
    async fn task_center_list_filters_and_paginates() {
        use crate::ports::TaskListFilter;
        let s = svc(StubBehavior::Accept { run_id: "r".into() });
        s.dispatch("d1", "t1", "a", "", &[], "CODEX", None, None, None).await.expect("d");
        s.dispatch("d1", "t2", "b", "", &[], "CLAUDE_CODE", None, None, None).await.expect("d");
        let c = s.dispatch("d1", "t3", "c", "", &[], "CODEX", None, None, None).await.expect("d");
        s.stop(&c.id, "manual").await.expect("stop");

        let all =
            s.list_tasks(&TaskListFilter { limit: 10, ..Default::default() }).await.expect("all");
        assert_eq!(all.total, 3);
        assert_eq!(all.items.len(), 3);
        assert_eq!(all.items[0].attempt.task_id, "t3");

        let active = s
            .list_tasks(&TaskListFilter { active_only: true, limit: 10, ..Default::default() })
            .await
            .expect("active");
        assert_eq!(active.total, 2);

        let pg = s
            .list_tasks(&TaskListFilter { limit: 1, offset: 1, ..Default::default() })
            .await
            .expect("pg");
        assert_eq!(pg.total, 3);
        assert_eq!(pg.items.len(), 1);
        assert_eq!(pg.items[0].attempt.task_id, "t2");
    }

    #[tokio::test]
    async fn stop_then_delete_terminal_only() {
        let s = svc(StubBehavior::Accept { run_id: "r".into() });
        let a = s.dispatch("d1", "t1", "x", "", &[], "CODEX", None, None, None).await.expect("d");
        assert!(matches!(s.delete(&a.id).await.unwrap_err(), DeliveryCmdError::Conflict(_)));
        let stopped = s.stop(&a.id, "").await.expect("stop");
        assert_eq!(stopped.status, AttemptStatus::Stopped);
        assert_eq!(stopped.error.as_deref(), Some("stopped by user"));
        assert!(matches!(s.stop(&a.id, "x").await.unwrap_err(), DeliveryCmdError::Conflict(_)));
        s.delete(&a.id).await.expect("delete");
        assert_eq!(s.get(&a.id).await.unwrap_err(), DeliveryCmdError::NotFound);
    }
}
