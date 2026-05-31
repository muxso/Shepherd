//! 跨上下文编排的组装根桥接:把 `orchestrator` 的 gateway 端口接到 task / verification 的真实
//! 服务,并把 delivery 的 `DeliveryObserver` 钩子桥接到编排器(驱动任务生命周期 + 回灌验证)。
//!
//! 这里是全工程唯一同时认识 delivery / task / verification / orchestrator 具体类型的地方 ——
//! 各业务上下文彼此仍不相互依赖。

use std::sync::Arc;

use async_trait::async_trait;

use delivery::domain::{AttemptStatus, DeliveryAttempt};
use delivery::ports::DeliveryObserver;
use orchestrator::application::{DeliveryFeedbackOrchestrator, DeliveryProgress};
use orchestrator::ports::{OrchError, TaskGateway, TaskTarget, VerificationGateway};
use task::application::{TaskCmdError, TaskService};
use task::domain::TaskStatus;
use verification::application::{VerificationCmdError, VerificationService};

/// task 上下文桥接:定位需求版本 + 推进任务生命周期。
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
            TaskTarget::Failed => TaskStatus::Failed,
        };
        match self.svc.advance_to(decomposition_id, task_id, status).await {
            Ok(_) => Ok(()),
            Err(e) => Err(OrchError::Gateway(format!("{e:?}"))),
        }
    }
}

/// verification 上下文桥接:查验证 + 同步覆盖链状态。
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
            Err(VerificationCmdError::NotFound) => Ok(()), // 验证已不存在 → 忽略
            Err(e) => Err(OrchError::Gateway(format!("{e:?}"))),
        }
    }
}

/// delivery 观察者 → 编排器:交付进度推进时驱动任务 + (终态)回灌验证。
struct OrchestratorObserver {
    orchestrator: Arc<DeliveryFeedbackOrchestrator>,
}

#[async_trait]
impl DeliveryObserver for OrchestratorObserver {
    async fn on_progress(&self, attempt: &DeliveryAttempt) {
        let progress = match attempt.status {
            AttemptStatus::Running => DeliveryProgress::Running,
            AttemptStatus::Delivered => DeliveryProgress::Delivered,
            AttemptStatus::Failed => DeliveryProgress::Failed,
            AttemptStatus::Dispatched => return, // 初始态不触发
        };
        // 尽力而为:编排失败不影响交付主流程。
        let _ = self
            .orchestrator
            .on_progress(&attempt.decomposition_id, &attempt.task_id, progress)
            .await;
    }
}

/// 组装交付编排观察者(供组装根挂到 `DeliveryService::with_observer`):
/// 交付进度 → 驱动任务生命周期 + 交付结果回灌验证。
pub fn delivery_observer(
    task: TaskService,
    verification: VerificationService,
) -> Arc<dyn DeliveryObserver> {
    let orchestrator = Arc::new(DeliveryFeedbackOrchestrator::new(
        Arc::new(TaskServiceGateway { svc: task }),
        Arc::new(VerificationServiceGateway { svc: verification }),
    ));
    Arc::new(OrchestratorObserver { orchestrator })
}
