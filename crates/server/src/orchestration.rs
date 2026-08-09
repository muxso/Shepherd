use std::sync::Arc;

use async_trait::async_trait;

use delivery::application::DeliveryService;
use delivery::domain::{AttemptStatus, DeliveryAttempt, ExecutorKind};
use delivery::ports::{AgentExecutor, DeliveryObserver, DispatchOutcome, NoopEventSink, WorkSpec};
use orchestrator::application::{DeliveryFeedbackOrchestrator, DeliveryProgress};
use orchestrator::ports::{
    DeliverableView, OrchError, Reviser, TaskGateway, TaskTarget, VerificationGateway,
};
use requirement::application::RequirementService;
use task::application::{TaskCmdError, TaskService};
use task::domain::TaskStatus;
use verification::application::{VerificationCmdError, VerificationService};

use crate::judge;
use crate::mcp_bus::{McpBus, McpEvent};

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
            Ok(d) => Ok(d.task(task_id).map(|t| t.acceptance_criteria.clone()).unwrap_or_default()),
            Err(TaskCmdError::DecompositionNotFound) => Ok(Vec::new()),
            Err(e) => Err(OrchError::Gateway(format!("{e:?}"))),
        }
    }
}

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
            // Revision finishes synchronously, no async callback, so attempt_id stays empty.
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
            target_runtime: None,
        };
        match self.executor.dispatch(&spec, &NoopEventSink).await {
            Ok(DispatchOutcome::Completed { deliverable }) => Ok(DeliverableView {
                kind: deliverable.kind.as_str().to_string(),
                reference: deliverable.reference,
                summary: deliverable.summary,
            }),
            Ok(_) => Err(OrchError::Gateway("revision produced no deliverable".into())),
            Err(e) => Err(OrchError::Gateway(format!("{e:?}"))),
        }
    }
}

/// `recorder` must be an **observer-free** DeliveryService, otherwise an Arc cycle forms.
struct OrchestratorObserver {
    orchestrator: Arc<DeliveryFeedbackOrchestrator>,
    recorder: DeliveryService,
    task: TaskService,
    requirements: RequirementService,
    bus: McpBus,
}

impl OrchestratorObserver {
    async fn try_deliver_requirement(&self, decomposition_id: &str) {
        let Ok(dec) = self.task.get(decomposition_id).await else { return };
        if dec.tasks.is_empty() || !dec.tasks.iter().all(|t| t.status == TaskStatus::Verified) {
            return;
        }
        match self.requirements.deliver(&dec.requirement_id, "orchestrator").await {
            Ok(_) => {
                tracing::info!(requirement = %dec.requirement_id, "all tasks verified → requirement auto-delivered (DELIVERED)");
                self.bus.publish(McpEvent {
                    kind: "requirement",
                    status: "delivered".into(),
                    attempt_id: String::new(),
                    task_id: String::new(),
                    message: format!("requirement {} delivered", dec.requirement_id),
                });
            }
            Err(e) => {
                tracing::warn!(requirement = %dec.requirement_id, "requirement auto-delivery failed (baseline may not be set): {e:?}")
            }
        }
    }
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
            // Dispatched-but-not-started / deliberately stopped: don't drive the verification gate.
            AttemptStatus::Dispatched | AttemptStatus::Stopped => return,
        };
        let dstatus = match attempt.status {
            AttemptStatus::Running => "running",
            AttemptStatus::Delivered => "delivered",
            _ => "failed",
        };
        self.bus.publish(McpEvent {
            kind: "delivery",
            status: dstatus.into(),
            attempt_id: attempt.id.clone(),
            task_id: attempt.task_id.clone(),
            message: String::new(),
        });
        if let Ok(outcome) = self
            .orchestrator
            .on_progress(&attempt.decomposition_id, &attempt.task_id, progress)
            .await
        {
            if let Some(v) = outcome.verdict {
                let msg = if v.passed {
                    format!("verification gate passed: {}", v.reason)
                } else {
                    format!("verification gate failed: {}", v.reason)
                };
                let _ = self.recorder.record_event(&attempt.id, "VERDICT", &msg, None).await;
                self.bus.publish(McpEvent {
                    kind: "verification",
                    status: if v.passed { "passed" } else { "failed" }.into(),
                    attempt_id: attempt.id.clone(),
                    task_id: attempt.task_id.clone(),
                    message: v.reason.clone(),
                });
            }
            if matches!(attempt.status, AttemptStatus::Delivered) {
                self.try_deliver_requirement(&attempt.decomposition_id).await;
            }
        }
    }
}

/// `recorder` should be an **observer-free** DeliveryService (avoids an Arc cycle).
pub fn delivery_observer(
    task: TaskService,
    verification: VerificationService,
    recorder: DeliveryService,
    executor: Arc<dyn AgentExecutor>,
    requirements: RequirementService,
    bus: McpBus,
) -> Arc<dyn DeliveryObserver> {
    let mut orchestrator = DeliveryFeedbackOrchestrator::new(
        Arc::new(TaskServiceGateway { svc: task.clone() }),
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
    Arc::new(OrchestratorObserver {
        orchestrator: Arc::new(orchestrator),
        recorder,
        task,
        requirements,
        bus,
    })
}
