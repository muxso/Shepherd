//! 跨上下文编排的组装根桥接:把 `orchestrator` 的 gateway 接到 task / verification 真实服务,
//! 把 delivery 的 `DeliveryObserver` 钩子桥接到编排器(驱动任务 + 验证门 + 回灌验证)。
//!
//! 全工程唯一同时认识 delivery / task / verification / orchestrator 具体类型的地方。

use std::sync::Arc;

use async_trait::async_trait;

use delivery::application::DeliveryService;
use delivery::domain::{AttemptStatus, DeliveryAttempt, ExecutorKind};
use delivery::ports::{
    AgentExecutor, DeliveryObserver, DispatchOutcome, NoopEventSink, WorkSpec,
};
use orchestrator::application::{DeliveryFeedbackOrchestrator, DeliveryProgress};
use orchestrator::ports::{
    DeliverableView, OrchError, Reviser, TaskGateway, TaskTarget, VerificationGateway,
};
use task::application::{TaskCmdError, TaskService};
use task::domain::TaskStatus;
use verification::application::{VerificationCmdError, VerificationService};

use crate::judge;

/// task 上下文桥接:定位需求版本 + 推进任务生命周期 + 取验收标准。
struct TaskServiceGateway {
    svc: TaskService,
}

#[async_trait]
impl TaskGateway for TaskServiceGateway {
    async fn requirement_of(&self, id: &str) -> Result<Option<(String, u32)>, OrchError> {
        match self.svc.get(id).await {
            Ok(d) => Ok(Some((d.requirement_id, d.requirement_version))),
            Err(TaskCmdError::DecompositionNotFound) => Ok(None),
            Err(e) => Err(OrchError::Gateway(format!("{e:?}"))),
        }
    }

    async fn advance_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
        target: TaskTarget,
    ) -> Result<(), OrchError> {
        let status = match target {
            TaskTarget::Running => TaskStatus::Running,
            TaskTarget::Delivered => TaskStatus::Delivered,
            TaskTarget::Verified => TaskStatus::Verified,
            TaskTarget::Failed => TaskStatus::Failed,
        };
        self.svc
            .advance_to(decomposition_id, task_id, status)
            .await
            .map(|_| ())
            .map_err(|e| OrchError::Gateway(format!("{e:?}")))
    }

    async fn task_criteria(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<String>, OrchError> {
        match self.svc.get(decomposition_id).await {
            Ok(d) => Ok(d
                .task(task_id)
                .map(|t| t.acceptance_criteria.clone())
                .unwrap_or_default()),
            Err(TaskCmdError::DecompositionNotFound) => Ok(Vec::new()),
            Err(e) => Err(OrchError::Gateway(format!("{e:?}"))),
        }
    }
}

/// verification 上下文桥接。
struct VerificationServiceGateway {
    svc: VerificationService,
}

#[async_trait]
impl VerificationGateway for VerificationServiceGateway {
    async fn find_verification(
        &self,
        requirement_id: &str,
        version: u32,
    ) -> Result<Option<String>, OrchError> {
        match self.svc.find_by_requirement_version(requirement_id, version).await {
            Ok(v) => Ok(v.map(|v| v.id)),
            Err(e) => Err(OrchError::Gateway(format!("{e:?}"))),
        }
    }

    async fn link(
        &self,
        verification_id: &str,
        decomposition_id: &str,
        task_id: &str,
        criteria_texts: &[String],
    ) -> Result<(), OrchError> {
        match self
            .svc
            .link_by_criteria_texts(verification_id, decomposition_id, task_id, criteria_texts)
            .await
        {
            Ok(_) => Ok(()),
            Err(VerificationCmdError::NotFound) => Ok(()),
            Err(e) => Err(OrchError::Gateway(format!("{e:?}"))),
        }
    }

    async fn sync(
        &self,
        verification_id: &str,
        decomposition_id: &str,
        task_id: &str,
        satisfied: bool,
    ) -> Result<(), OrchError> {
        match self.svc.sync_task(verification_id, decomposition_id, task_id, satisfied).await {
            Ok(_) => Ok(()),
            Err(VerificationCmdError::NotFound) => Ok(()),
            Err(e) => Err(OrchError::Gateway(format!("{e:?}"))),
        }
    }
}

/// 修订者桥接:验证门不通过时,用执行者(LLM/agent)据反馈重做,产出新交付物。
struct ExecutorReviser {
    executor: Arc<dyn AgentExecutor>,
}

#[async_trait]
impl Reviser for ExecutorReviser {
    async fn revise(
        &self,
        decomposition_id: &str,
        task_id: &str,
        criteria: &[String],
        previous: &DeliverableView,
        feedback: &str,
    ) -> Result<DeliverableView, OrchError> {
        let spec = WorkSpec {
            // 修订走同步收尾(LLM/sync executor),不依赖异步回调,attempt_id 暂置空。
            attempt_id: String::new(),
            decomposition_id: decomposition_id.to_string(),
            task_id: task_id.to_string(),
            title: format!("修订任务 {task_id}"),
            description: previous.summary.clone(),
            acceptance_criteria: criteria.to_vec(),
            executor: ExecutorKind::ClaudeCode,
            context: None,
            instructions: Some(format!(
                "上一轮交付未通过验证门,请据反馈修正后重做。\n反馈: {feedback}"
            )),
        };
        match self.executor.dispatch(&spec, &NoopEventSink).await {
            Ok(DispatchOutcome::Completed { deliverable }) => Ok(DeliverableView {
                kind: deliverable.kind.as_str().to_string(),
                reference: deliverable.reference,
                summary: deliverable.summary,
            }),
            Ok(_) => Err(OrchError::Gateway("修订未产出交付物".into())),
            Err(e) => Err(OrchError::Gateway(format!("{e:?}"))),
        }
    }
}

/// delivery 观察者 → 编排器:交付进度推进时驱动任务 + 验证门 +(终态)回灌验证;
/// 并把验证门裁决记入该尝试的审计事件(`recorder` 为无观察者的 DeliveryService,避免 Arc 环)。
struct OrchestratorObserver {
    orchestrator: Arc<DeliveryFeedbackOrchestrator>,
    recorder: DeliveryService,
}

#[async_trait]
impl DeliveryObserver for OrchestratorObserver {
    async fn on_progress(&self, attempt: &DeliveryAttempt) {
        let progress = match attempt.status {
            AttemptStatus::Running => DeliveryProgress::Running,
            AttemptStatus::Delivered => {
                let deliverable = attempt
                    .deliverable
                    .as_ref()
                    .map(|d| DeliverableView {
                        kind: d.kind.as_str().to_string(),
                        reference: d.reference.clone(),
                        summary: d.summary.clone(),
                    })
                    .unwrap_or(DeliverableView {
                        kind: String::new(),
                        reference: String::new(),
                        summary: String::new(),
                    });
                DeliveryProgress::Delivered { deliverable }
            }
            AttemptStatus::Failed => DeliveryProgress::Failed,
            AttemptStatus::Dispatched => return,
        };
        if let Ok(outcome) =
            self.orchestrator.on_progress(&attempt.decomposition_id, &attempt.task_id, progress).await
        {
            // 把验证门裁决记入交付审计轨迹(尽力而为)。
            if let Some(v) = outcome.verdict {
                let msg = if v.passed {
                    format!("验证门通过: {}", v.reason)
                } else {
                    format!("验证门未通过: {}", v.reason)
                };
                let _ = self.recorder.record_event(&attempt.id, "VERDICT", &msg, None).await;
            }
        }
    }
}

/// 组装交付编排观察者:驱动任务生命周期 + 验证门(judge)+ 回灌验证 + 裁决记审计。
/// `recorder` 应为**无观察者**的 DeliveryService(避免 Arc 环);judge 由 `judge::build_judge()` 按环境选择。
/// `executor` 用于自纠正迭代:设 `SHEPHERD_MAX_REVISIONS=N`(N>0)则验证门不通过时,
/// 据反馈最多重做 N 轮(executor 即交付执行者:LLM/agent)。默认 0 = 不迭代(行为不变)。
pub fn delivery_observer(
    task: TaskService,
    verification: VerificationService,
    recorder: DeliveryService,
    executor: Arc<dyn AgentExecutor>,
) -> Arc<dyn DeliveryObserver> {
    let mut orchestrator = DeliveryFeedbackOrchestrator::new(
        Arc::new(TaskServiceGateway { svc: task }),
        Arc::new(VerificationServiceGateway { svc: verification }),
        judge::build_judge(),
    );
    let max_revisions = std::env::var("SHEPHERD_MAX_REVISIONS")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    if max_revisions > 0 {
        orchestrator =
            orchestrator.with_revision(Arc::new(ExecutorReviser { executor }), max_revisions);
    }
    Arc::new(OrchestratorObserver { orchestrator: Arc::new(orchestrator), recorder })
}
