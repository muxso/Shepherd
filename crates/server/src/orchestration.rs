//! 跨上下文编排的组装根桥接:把 `orchestrator` 的 gateway 接到 task / verification 真实服务,
//! 把 delivery 的 `DeliveryObserver` 钩子桥接到编排器(驱动任务 + 验证门 + 回灌验证)。
//!
//! 全工程唯一同时认识 delivery / task / verification / orchestrator 具体类型的地方。

use std::sync::Arc;

use async_trait::async_trait;

use delivery::domain::{AttemptStatus, DeliveryAttempt};
use delivery::ports::DeliveryObserver;
use orchestrator::application::{DeliveryFeedbackOrchestrator, DeliveryProgress};
use orchestrator::ports::{DeliverableView, OrchError, TaskGateway, TaskTarget, VerificationGateway};
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

/// delivery 观察者 → 编排器:交付进度推进时驱动任务 + 验证门 +(终态)回灌验证。
struct OrchestratorObserver {
    orchestrator: Arc<DeliveryFeedbackOrchestrator>,
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
        let _ = self.orchestrator.on_progress(&attempt.decomposition_id, &attempt.task_id, progress).await;
    }
}

/// 组装交付编排观察者:驱动任务生命周期 + 验证门(judge)+ 回灌验证。
/// judge 由 `judge::build_judge()` 按环境选择(默认 AcceptAll,设 SHEPHERD_JUDGE_URL 用 HTTP/LLM judge)。
pub fn delivery_observer(
    task: TaskService,
    verification: VerificationService,
) -> Arc<dyn DeliveryObserver> {
    let orchestrator = Arc::new(DeliveryFeedbackOrchestrator::new(
        Arc::new(TaskServiceGateway { svc: task }),
        Arc::new(VerificationServiceGateway { svc: verification }),
        judge::build_judge(),
    ));
    Arc::new(OrchestratorObserver { orchestrator })
}
