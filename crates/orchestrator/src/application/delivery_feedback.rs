//! 用例:交付进度 → 驱动任务生命周期 + 验证门 + 回灌验证。
//!
//! 交付**成功**时不再"交付即 Verified",而是先过**验证门(judge)**:据任务验收标准评判交付物,
//! 通过 → 任务推进到 Verified + 验证覆盖链 satisfied=true;不通过 → 任务置 Failed + satisfied=false
//! (缺口保留)。Running 仅推进任务;Failed 直接置失败。

use std::sync::Arc;

use crate::ports::{
    DeliverableView, Judge, OrchError, TaskGateway, TaskTarget, Verdict, VerificationGateway,
};

/// 交付进度。`Delivered` 携带交付物以供验证门评判。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryProgress {
    Running,
    Delivered { deliverable: DeliverableView },
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationSync {
    NotApplicable,
    NoDecomposition,
    NoVerification,
    Synced { verification_id: String, satisfied: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackOutcome {
    pub task_advanced: bool,
    /// 交付成功时的验证门裁决(Running/Failed 为 None)。
    pub verdict: Option<Verdict>,
    pub verification: VerificationSync,
}

#[derive(Clone)]
pub struct DeliveryFeedbackOrchestrator {
    task: Arc<dyn TaskGateway>,
    verification: Arc<dyn VerificationGateway>,
    judge: Arc<dyn Judge>,
}

impl DeliveryFeedbackOrchestrator {
    pub fn new(
        task: Arc<dyn TaskGateway>,
        verification: Arc<dyn VerificationGateway>,
        judge: Arc<dyn Judge>,
    ) -> Self {
        Self { task, verification, judge }
    }

    pub async fn on_progress(
        &self,
        decomposition_id: &str,
        task_id: &str,
        progress: DeliveryProgress,
    ) -> Result<FeedbackOutcome, OrchError> {
        // 决定:任务推进目标 + 是否满足(回灌验证)+ 裁决。
        let (target, satisfied, verdict): (Option<TaskTarget>, Option<bool>, Option<Verdict>) =
            match progress {
                DeliveryProgress::Running => (Some(TaskTarget::Running), None, None),
                DeliveryProgress::Failed => (Some(TaskTarget::Failed), Some(false), None),
                DeliveryProgress::Delivered { deliverable } => {
                    // 交付已完成:先把任务推进到 Delivered(走完 happy path),再过验证门。
                    let _ = self
                        .task
                        .advance_task(decomposition_id, task_id, TaskTarget::Delivered)
                        .await;
                    // 验证门:据任务验收标准评判交付物。
                    let criteria = self.task.task_criteria(decomposition_id, task_id).await?;
                    let v = self.judge.judge(&criteria, &deliverable).await;
                    if v.passed {
                        (Some(TaskTarget::Verified), Some(true), Some(v))
                    } else {
                        (Some(TaskTarget::Failed), Some(false), Some(v))
                    }
                }
            };

        // 驱动任务(尽力而为)。
        let task_advanced = match target {
            Some(t) => self.task.advance_task(decomposition_id, task_id, t).await.is_ok(),
            None => false,
        };

        // 回灌验证(仅终态:satisfied 为 Some 时)。
        let verification = match satisfied {
            None => VerificationSync::NotApplicable,
            Some(sat) => match self.task.requirement_of(decomposition_id).await? {
                None => VerificationSync::NoDecomposition,
                Some((req_id, version)) => {
                    match self.verification.find_verification(&req_id, version).await? {
                        None => VerificationSync::NoVerification,
                        Some(vid) => {
                            self.verification.sync(&vid, decomposition_id, task_id, sat).await?;
                            VerificationSync::Synced { verification_id: vid, satisfied: sat }
                        }
                    }
                }
            },
        };

        Ok(FeedbackOutcome { task_advanced, verdict, verification })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judges::{AcceptAllJudge, RuleJudge};
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeTask {
        map: Vec<(String, String, u32)>,
        criteria: Vec<String>,
        advanced: Mutex<Vec<(String, TaskTarget)>>,
    }
    #[async_trait]
    impl TaskGateway for FakeTask {
        async fn requirement_of(&self, id: &str) -> Result<Option<(String, u32)>, OrchError> {
            Ok(self.map.iter().find(|(d, _, _)| d == id).map(|(_, r, v)| (r.clone(), *v)))
        }
        async fn advance_task(&self, _d: &str, t: &str, target: TaskTarget) -> Result<(), OrchError> {
            self.advanced.lock().unwrap().push((t.into(), target));
            Ok(())
        }
        async fn task_criteria(&self, _d: &str, _t: &str) -> Result<Vec<String>, OrchError> {
            Ok(self.criteria.clone())
        }
    }

    #[derive(Default)]
    struct FakeVerif {
        found: Option<(String, u32, String)>,
        synced: Mutex<Vec<(String, bool)>>,
    }
    #[async_trait]
    impl VerificationGateway for FakeVerif {
        async fn find_verification(&self, req: &str, ver: u32) -> Result<Option<String>, OrchError> {
            Ok(self.found.as_ref().filter(|(r, v, _)| r == req && *v == ver).map(|(_, _, id)| id.clone()))
        }
        async fn sync(&self, vid: &str, _d: &str, _t: &str, s: bool) -> Result<(), OrchError> {
            self.synced.lock().unwrap().push((vid.into(), s));
            Ok(())
        }
    }

    fn dv(reference: &str, summary: &str) -> DeliverableView {
        DeliverableView { kind: "DIFF".into(), reference: reference.into(), summary: summary.into() }
    }

    #[tokio::test]
    async fn delivered_passing_gate_verifies_and_syncs_true() {
        let task = Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], ..Default::default() });
        let verif = Arc::new(FakeVerif { found: Some(("req1".into(), 1, "v1".into())), ..Default::default() });
        let orch = DeliveryFeedbackOrchestrator::new(task.clone(), verif.clone(), Arc::new(RuleJudge));

        let out = orch
            .on_progress("d1", "t1", DeliveryProgress::Delivered { deliverable: dv("branch:x", "done") })
            .await
            .expect("ok");
        assert!(out.verdict.as_ref().unwrap().passed);
        assert_eq!(task.advanced.lock().unwrap().last().unwrap().1, TaskTarget::Verified);
        assert_eq!(verif.synced.lock().unwrap()[0].1, true);
    }

    #[tokio::test]
    async fn delivered_failing_gate_fails_task_and_syncs_false() {
        let task = Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], ..Default::default() });
        let verif = Arc::new(FakeVerif { found: Some(("req1".into(), 1, "v1".into())), ..Default::default() });
        let orch = DeliveryFeedbackOrchestrator::new(task.clone(), verif.clone(), Arc::new(RuleJudge));

        // 交付物缺 summary → 规则门不通过
        let out = orch
            .on_progress("d1", "t1", DeliveryProgress::Delivered { deliverable: dv("branch:x", "") })
            .await
            .expect("ok");
        assert!(!out.verdict.as_ref().unwrap().passed);
        assert_eq!(task.advanced.lock().unwrap().last().unwrap().1, TaskTarget::Failed);
        assert_eq!(verif.synced.lock().unwrap()[0].1, false);
    }

    #[tokio::test]
    async fn accept_all_keeps_legacy_behavior() {
        let task = Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], ..Default::default() });
        let verif = Arc::new(FakeVerif { found: Some(("req1".into(), 1, "v1".into())), ..Default::default() });
        let orch = DeliveryFeedbackOrchestrator::new(task.clone(), verif, Arc::new(AcceptAllJudge));
        // 即便交付物空,AcceptAll 也通过 → Verified
        let out = orch
            .on_progress("d1", "t1", DeliveryProgress::Delivered { deliverable: dv("", "") })
            .await
            .expect("ok");
        assert!(out.verdict.unwrap().passed);
        assert_eq!(task.advanced.lock().unwrap().last().unwrap().1, TaskTarget::Verified);
    }

    #[tokio::test]
    async fn running_advances_only() {
        let task = Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], ..Default::default() });
        let orch = DeliveryFeedbackOrchestrator::new(task.clone(), Arc::new(FakeVerif::default()), Arc::new(AcceptAllJudge));
        let out = orch.on_progress("d1", "t1", DeliveryProgress::Running).await.expect("ok");
        assert_eq!(out.verification, VerificationSync::NotApplicable);
        assert!(out.verdict.is_none());
        assert_eq!(task.advanced.lock().unwrap()[0].1, TaskTarget::Running);
    }
}
