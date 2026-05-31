//! 用例:交付编排 —— 派发任务给执行者并按 `DispatchOutcome` 推进尝试;异步回调收尾。

use std::sync::Arc;

use crate::domain::{
    AttemptStatus, Deliverable, DeliverableKind, DeliveryAttempt, DeliveryError, ExecutorKind,
};
use crate::ports::{
    AgentExecutor, DeliveryObserver, DeliveryRepository, DispatchOutcome, ExecError, RepoError,
    WorkSpec,
};

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

    /// 尝试落终态(Delivered/Failed)时通知观察者(尽力而为)。
    async fn notify_settled(&self, attempt: &DeliveryAttempt) {
        if matches!(attempt.status, AttemptStatus::Delivered | AttemptStatus::Failed) {
            if let Some(o) = &self.observer {
                o.on_settled(attempt).await;
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
        };

        match self.executor.dispatch(&spec).await {
            Ok(DispatchOutcome::Accepted { run_id }) => attempt.start_running(&run_id)?,
            Ok(DispatchOutcome::Completed { deliverable }) => attempt.deliver(deliverable)?,
            Err(ExecError::Backend(msg)) => attempt.fail(&msg)?,
        }
        self.repo.save(&attempt).await?;
        self.notify_settled(&attempt).await;
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
        self.notify_settled(&a).await;
        Ok(a)
    }

    /// 异步回调:执行者失败。
    pub async fn fail(&self, id: &str, error: &str) -> Result<DeliveryAttempt, DeliveryCmdError> {
        let mut a = self.get(id).await?;
        a.fail(error)?;
        self.repo.save(&a).await?;
        self.notify_settled(&a).await;
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
            .dispatch("d1", "t1", "build login", "do it", &["c1".into()], "CLAUDE_CODE", None)
            .await
            .expect("dispatch");
        assert_eq!(a.status, AttemptStatus::Delivered);
        assert_eq!(a.deliverable.as_ref().expect("d").kind, DeliverableKind::PullRequest);
    }

    #[tokio::test]
    async fn async_executor_goes_running_then_callback_completes() {
        let s = svc(StubBehavior::Accept { run_id: "run-1".into() });
        let a = s
            .dispatch("d1", "t1", "build", "", &[], "CODEX", None)
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
        let a = s.dispatch("d1", "t1", "build", "", &[], "CLAUDE_CODE", None).await.expect("dispatch");
        assert_eq!(a.status, AttemptStatus::Failed);
        assert_eq!(a.error.as_deref(), Some("spawn failed"));
    }

    #[tokio::test]
    async fn unknown_executor_is_validation() {
        let s = svc(StubBehavior::Accept { run_id: "r".into() });
        assert!(matches!(
            s.dispatch("d1", "t1", "x", "", &[], "GPT5", None).await.unwrap_err(),
            DeliveryCmdError::Validation(_)
        ));
    }

    #[tokio::test]
    async fn callback_on_terminal_is_conflict() {
        let s = svc(StubBehavior::Complete { deliverable: deliverable() });
        let a = s.dispatch("d1", "t1", "x", "", &[], "CODEX", None).await.expect("dispatch");
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
            async fn on_settled(&self, a: &DeliveryAttempt) {
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
        svc.dispatch("d1", "t1", "x", "", &[], "CLAUDE_CODE", None).await.expect("dispatch");
        assert_eq!(spy.settled.lock().unwrap().as_slice(), &[("t1".into(), "DELIVERED".into())]);
    }

    #[tokio::test]
    async fn observer_not_notified_on_non_terminal() {
        use crate::ports::DeliveryObserver;
        use async_trait::async_trait;
        use std::sync::Mutex;

        #[derive(Default)]
        struct Spy {
            n: Mutex<usize>,
        }
        #[async_trait]
        impl DeliveryObserver for Spy {
            async fn on_settled(&self, _a: &DeliveryAttempt) {
                *self.n.lock().unwrap() += 1;
            }
        }

        let spy = Arc::new(Spy::default());
        let svc = DeliveryService::new(
            Arc::new(InMemoryDeliveryRepository::new()),
            Arc::new(StubAgentExecutor::new(StubBehavior::Accept { run_id: "r".into() })),
        )
        .with_observer(spy.clone());

        // 异步接单 → Running(非终态)→ 不通知
        svc.dispatch("d1", "t1", "x", "", &[], "CODEX", None).await.expect("dispatch");
        assert_eq!(*spy.n.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn list_by_task_and_not_found() {
        let s = svc(StubBehavior::Accept { run_id: "r".into() });
        s.dispatch("d1", "t1", "x", "", &[], "CODEX", None).await.expect("d");
        s.dispatch("d1", "t1", "y", "", &[], "CODEX", None).await.expect("d");
        assert_eq!(s.list_by_task("d1", "t1").await.expect("list").len(), 2);
        assert_eq!(s.get("ghost").await.unwrap_err(), DeliveryCmdError::NotFound);
    }
}
