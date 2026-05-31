//! 跨上下文编排的组装根桥接:把 `orchestrator` 的 gateway 端口接到 task / verification 的真实
//! 服务,并把 delivery 的 `DeliveryObserver` 钩子桥接到"交付结果回灌验证"编排器。
//!
//! 这里是全工程唯一同时认识 delivery / task / verification / orchestrator 具体类型的地方 ——
//! 各业务上下文彼此仍不相互依赖。

use std::sync::Arc;

use async_trait::async_trait;

use delivery::domain::{AttemptStatus, DeliveryAttempt};
use delivery::ports::DeliveryObserver;
use orchestrator::application::DeliveryFeedbackOrchestrator;
use orchestrator::ports::{DecompositionGateway, OrchError, VerificationGateway};
use task::application::{TaskCmdError, TaskService};
use verification::application::{VerificationCmdError, VerificationService};

/// 把 task 拆分图映射到它归属的需求版本。
struct TaskDecompositionGateway {
    svc: TaskService,
}

#[async_trait]
impl DecompositionGateway for TaskDecompositionGateway {
    async fn requirement_of(&self, id: &str) -> Result<Option<(String, u32)>, OrchError> {
        match self.svc.get(id).await {
            Ok(d) => Ok(Some((d.requirement_id, d.requirement_version))),
            Err(TaskCmdError::DecompositionNotFound) => Ok(None),
            Err(e) => Err(OrchError::Gateway(format!("{e:?}"))),
        }
    }
}

/// 查验证 + 同步覆盖链状态。
struct VerificationFeedbackGateway {
    svc: VerificationService,
}

#[async_trait]
impl VerificationGateway for VerificationFeedbackGateway {
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
            // 验证已不存在 → 忽略(尽力而为)
            Err(VerificationCmdError::NotFound) => Ok(()),
            Err(e) => Err(OrchError::Gateway(format!("{e:?}"))),
        }
    }
}

/// delivery 观察者 → 编排器:交付落终态即回灌验证(Delivered⇒satisfied,Failed⇒unsatisfied)。
struct OrchestratorObserver {
    orchestrator: Arc<DeliveryFeedbackOrchestrator>,
}

#[async_trait]
impl DeliveryObserver for OrchestratorObserver {
    async fn on_settled(&self, attempt: &DeliveryAttempt) {
        let delivered = attempt.status == AttemptStatus::Delivered;
        // 尽力而为:回灌失败不影响交付主流程。
        let _ = self
            .orchestrator
            .on_settled(&attempt.decomposition_id, &attempt.task_id, delivered)
            .await;
    }
}

/// 组装"交付结果回灌验证"的 delivery 观察者(供组装根挂到 `DeliveryService::with_observer`)。
pub fn delivery_observer(
    task: TaskService,
    verification: VerificationService,
) -> Arc<dyn DeliveryObserver> {
    let orchestrator = Arc::new(DeliveryFeedbackOrchestrator::new(
        Arc::new(TaskDecompositionGateway { svc: task }),
        Arc::new(VerificationFeedbackGateway { svc: verification }),
    ));
    Arc::new(OrchestratorObserver { orchestrator })
}
