//! 用例:交付进度 → 驱动任务生命周期 + 验证门 + 回灌验证。
//!
//! 交付**成功**时不再"交付即 Verified",而是先过**验证门(judge)**:据任务验收标准评判交付物,
//! 通过 → 任务推进到 Verified + 验证覆盖链 satisfied=true;不通过 → 任务置 Failed + satisfied=false
//! (缺口保留)。Running 仅推进任务;Failed 直接置失败。

use std::sync::Arc;

use crate::ports::{
    DeliverableView, Judge, OrchError, Reviser, TaskGateway, TaskTarget, Verdict,
    VerificationGateway,
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
    /// 自纠正迭代次数(验证门不通过后据反馈重做的轮数;无修订者/一次通过则 0)。
    pub revisions: u32,
}

#[derive(Clone)]
pub struct DeliveryFeedbackOrchestrator {
    task: Arc<dyn TaskGateway>,
    verification: Arc<dyn VerificationGateway>,
    judge: Arc<dyn Judge>,
    /// 可选自纠正:验证门不通过时据反馈重做,最多 `max_revisions` 轮。
    reviser: Option<Arc<dyn Reviser>>,
    max_revisions: u32,
}

impl DeliveryFeedbackOrchestrator {
    pub fn new(
        task: Arc<dyn TaskGateway>,
        verification: Arc<dyn VerificationGateway>,
        judge: Arc<dyn Judge>,
    ) -> Self {
        Self { task, verification, judge, reviser: None, max_revisions: 0 }
    }

    /// 启用自纠正迭代:验证门不通过 → 据反馈调 reviser 重做并复判,最多 `max_revisions` 轮。
    pub fn with_revision(mut self, reviser: Arc<dyn Reviser>, max_revisions: u32) -> Self {
        self.reviser = Some(reviser);
        self.max_revisions = max_revisions;
        self
    }

    pub async fn on_progress(
        &self,
        decomposition_id: &str,
        task_id: &str,
        progress: DeliveryProgress,
    ) -> Result<FeedbackOutcome, OrchError> {
        // 决定:任务推进目标 + 是否满足(回灌验证)+ 裁决 + 任务验收标准(用于自动建链)。
        let mut revisions = 0u32;
        let (target, satisfied, verdict, criteria): (
            Option<TaskTarget>,
            Option<bool>,
            Option<Verdict>,
            Vec<String>,
        ) = match progress {
            DeliveryProgress::Running => (Some(TaskTarget::Running), None, None, Vec::new()),
            DeliveryProgress::Failed => {
                // 失败也建链(覆盖但未验证 = Unverified 缺口,比 Uncovered 更准确)。
                let criteria =
                    self.task.task_criteria(decomposition_id, task_id).await.unwrap_or_default();
                (Some(TaskTarget::Failed), Some(false), None, criteria)
            }
            DeliveryProgress::Delivered { deliverable } => {
                // 交付已完成:先把任务推进到 Delivered(走完 happy path),再过验证门。
                let _ = self
                    .task
                    .advance_task(decomposition_id, task_id, TaskTarget::Delivered)
                    .await;
                // 验证门:据任务验收标准评判交付物。
                let criteria = self.task.task_criteria(decomposition_id, task_id).await?;
                let mut current = deliverable;
                let mut v = self.judge.judge(&criteria, &current).await;
                // 自纠正迭代:不通过 → 据反馈重做并复判,最多 max_revisions 轮。
                if let Some(reviser) = &self.reviser {
                    while !v.passed && revisions < self.max_revisions {
                        match reviser
                            .revise(decomposition_id, task_id, &criteria, &current, &v.reason)
                            .await
                        {
                            Ok(next) => {
                                current = next;
                                v = self.judge.judge(&criteria, &current).await;
                                revisions += 1;
                            }
                            Err(_) => break, // 重做失败 → 停止迭代,按当前裁决落地
                        }
                    }
                }
                if v.passed {
                    (Some(TaskTarget::Verified), Some(true), Some(v), criteria)
                } else {
                    (Some(TaskTarget::Failed), Some(false), Some(v), criteria)
                }
            }
        };

        // 驱动任务(尽力而为)。
        let task_advanced = match target {
            Some(t) => self.task.advance_task(decomposition_id, task_id, t).await.is_ok(),
            None => false,
        };

        // 回灌验证(仅终态:satisfied 为 Some 时)。先按验收标准文本**自动建链**,再 sync ——
        // 否则覆盖链为空,sync 无链可更,完整性报告永远停在 UNCOVERED。
        let verification = match satisfied {
            None => VerificationSync::NotApplicable,
            Some(sat) => match self.task.requirement_of(decomposition_id).await? {
                None => VerificationSync::NoDecomposition,
                Some((req_id, version)) => {
                    match self.verification.find_verification(&req_id, version).await? {
                        None => VerificationSync::NoVerification,
                        Some(vid) => {
                            self.verification
                                .link(&vid, decomposition_id, task_id, &criteria)
                                .await?;
                            self.verification.sync(&vid, decomposition_id, task_id, sat).await?;
                            VerificationSync::Synced { verification_id: vid, satisfied: sat }
                        }
                    }
                }
            },
        };

        Ok(FeedbackOutcome { task_advanced, verdict, verification, revisions })
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
        linked: Mutex<Vec<(String, Vec<String>)>>,
        synced: Mutex<Vec<(String, bool)>>,
    }
    #[async_trait]
    impl VerificationGateway for FakeVerif {
        async fn find_verification(&self, req: &str, ver: u32) -> Result<Option<String>, OrchError> {
            Ok(self.found.as_ref().filter(|(r, v, _)| r == req && *v == ver).map(|(_, _, id)| id.clone()))
        }
        async fn link(&self, vid: &str, _d: &str, t: &str, texts: &[String]) -> Result<(), OrchError> {
            self.linked.lock().unwrap().push((format!("{vid}/{t}"), texts.to_vec()));
            Ok(())
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
    async fn terminal_auto_links_task_criteria_before_sync() {
        // 任务带验收标准 → 编排器应在 sync 前用这些文本自动建链。
        let task = Arc::new(FakeTask {
            map: vec![("d1".into(), "req1".into(), 1)],
            criteria: vec!["登录成功".into()],
            ..Default::default()
        });
        let verif = Arc::new(FakeVerif { found: Some(("req1".into(), 1, "v1".into())), ..Default::default() });
        let orch = DeliveryFeedbackOrchestrator::new(task, verif.clone(), Arc::new(AcceptAllJudge));

        orch.on_progress("d1", "t1", DeliveryProgress::Delivered { deliverable: dv("b", "d") })
            .await
            .expect("ok");
        // 建链发生且携带任务验收标准文本;且发生在 sync 之前。
        let linked = verif.linked.lock().unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0], ("v1/t1".to_string(), vec!["登录成功".to_string()]));
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

    /// 修订者:前 `fix_after` 次重做仍空 summary(判不过),之后产出带 summary 的交付物(判通过)。
    struct FakeReviser {
        calls: Mutex<u32>,
        fix_after: u32,
    }
    #[async_trait]
    impl crate::ports::Reviser for FakeReviser {
        async fn revise(
            &self,
            _d: &str,
            _t: &str,
            _c: &[String],
            _prev: &DeliverableView,
            _feedback: &str,
        ) -> Result<DeliverableView, OrchError> {
            let mut n = self.calls.lock().unwrap();
            *n += 1;
            if *n >= self.fix_after {
                Ok(dv("branch:fixed", "已据反馈补齐"))
            } else {
                Ok(dv("branch:retry", "")) // 仍缺 summary → RuleJudge 不通过
            }
        }
    }

    #[tokio::test]
    async fn revision_loop_fixes_and_verifies() {
        let task = Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], ..Default::default() });
        let verif = Arc::new(FakeVerif { found: Some(("req1".into(), 1, "v1".into())), ..Default::default() });
        // 第 1 轮重做即补齐(fix_after=1)→ 1 次修订后通过。
        let reviser = Arc::new(FakeReviser { calls: Mutex::new(0), fix_after: 1 });
        let orch = DeliveryFeedbackOrchestrator::new(task.clone(), verif.clone(), Arc::new(RuleJudge))
            .with_revision(reviser, 3);
        // 初始交付物缺 summary → 首判不通过 → 触发修订。
        let out = orch
            .on_progress("d1", "t1", DeliveryProgress::Delivered { deliverable: dv("branch:x", "") })
            .await
            .expect("ok");
        assert!(out.verdict.as_ref().unwrap().passed);
        assert_eq!(out.revisions, 1);
        assert_eq!(task.advanced.lock().unwrap().last().unwrap().1, TaskTarget::Verified);
        assert_eq!(verif.synced.lock().unwrap().last().unwrap().1, true);
    }

    #[tokio::test]
    async fn revision_loop_exhausts_then_fails() {
        let task = Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], ..Default::default() });
        let verif = Arc::new(FakeVerif { found: Some(("req1".into(), 1, "v1".into())), ..Default::default() });
        // 永不补齐(fix_after 极大)→ 用尽 2 轮仍失败。
        let reviser = Arc::new(FakeReviser { calls: Mutex::new(0), fix_after: 99 });
        let orch = DeliveryFeedbackOrchestrator::new(task.clone(), verif.clone(), Arc::new(RuleJudge))
            .with_revision(reviser, 2);
        let out = orch
            .on_progress("d1", "t1", DeliveryProgress::Delivered { deliverable: dv("branch:x", "") })
            .await
            .expect("ok");
        assert!(!out.verdict.as_ref().unwrap().passed);
        assert_eq!(out.revisions, 2); // 用尽上限
        assert_eq!(task.advanced.lock().unwrap().last().unwrap().1, TaskTarget::Failed);
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
