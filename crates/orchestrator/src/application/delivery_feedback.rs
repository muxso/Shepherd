use std::sync::Arc;

use crate::ports::{
    DeliverableView, Judge, OrchError, Reviser, TaskGateway, TaskTarget, Verdict,
    VerificationGateway,
};

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
    pub verdict: Option<Verdict>,
    pub verification: VerificationSync,
    pub revisions: u32,
}

#[derive(Clone)]
pub struct DeliveryFeedbackOrchestrator {
    task: Arc<dyn TaskGateway>,
    verification: Arc<dyn VerificationGateway>,
    judge: Arc<dyn Judge>,
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
        let mut revisions = 0u32;
        let (target, satisfied, verdict, criteria): (
            Option<TaskTarget>,
            Option<bool>,
            Option<Verdict>,
            Vec<String>,
        ) = match progress {
            DeliveryProgress::Running => (Some(TaskTarget::Running), None, None, Vec::new()),
            DeliveryProgress::Failed => {
                let criteria =
                    self.task.task_criteria(decomposition_id, task_id).await.unwrap_or_default();
                (Some(TaskTarget::Failed), Some(false), None, criteria)
            }
            DeliveryProgress::Delivered { deliverable } => {
                let _ =
                    self.task.advance_task(decomposition_id, task_id, TaskTarget::Delivered).await;
                let criteria = self.task.task_criteria(decomposition_id, task_id).await?;
                let mut current = deliverable;
                let mut v = self.judge.judge(&criteria, &current).await;
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
                            Err(_) => break,
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

        let task_advanced = match target {
            Some(t) => self.task.advance_task(decomposition_id, task_id, t).await.is_ok(),
            None => false,
        };

        // 须先 link 再 sync:否则覆盖链为空,sync 无链可更,完整性报告永远停在 UNCOVERED。
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
        async fn advance_task(
            &self,
            _d: &str,
            t: &str,
            target: TaskTarget,
        ) -> Result<(), OrchError> {
            self.advanced
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((t.into(), target));
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
        async fn find_verification(
            &self,
            req: &str,
            ver: u32,
        ) -> Result<Option<String>, OrchError> {
            Ok(self
                .found
                .as_ref()
                .filter(|(r, v, _)| r == req && *v == ver)
                .map(|(_, _, id)| id.clone()))
        }
        async fn link(
            &self,
            vid: &str,
            _d: &str,
            t: &str,
            texts: &[String],
        ) -> Result<(), OrchError> {
            self.linked
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((format!("{vid}/{t}"), texts.to_vec()));
            Ok(())
        }
        async fn sync(&self, vid: &str, _d: &str, _t: &str, s: bool) -> Result<(), OrchError> {
            self.synced
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((vid.into(), s));
            Ok(())
        }
    }

    fn dv(reference: &str, summary: &str) -> DeliverableView {
        DeliverableView {
            kind: "DIFF".into(),
            reference: reference.into(),
            summary: summary.into(),
        }
    }

    #[tokio::test]
    async fn delivered_passing_gate_verifies_and_syncs_true() {
        let task =
            Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], ..Default::default() });
        let verif = Arc::new(FakeVerif {
            found: Some(("req1".into(), 1, "v1".into())),
            ..Default::default()
        });
        let orch =
            DeliveryFeedbackOrchestrator::new(task.clone(), verif.clone(), Arc::new(RuleJudge));

        let out = orch
            .on_progress(
                "d1",
                "t1",
                DeliveryProgress::Delivered { deliverable: dv("branch:x", "done") },
            )
            .await
            .expect("ok");
        assert!(out.verdict.as_ref().unwrap().passed);
        assert_eq!(
            task.advanced
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .last()
                .unwrap()
                .1,
            TaskTarget::Verified
        );
        assert!(verif.synced.lock().unwrap_or_else(std::sync::PoisonError::into_inner)[0].1);
    }

    #[tokio::test]
    async fn terminal_auto_links_task_criteria_before_sync() {
        let task = Arc::new(FakeTask {
            map: vec![("d1".into(), "req1".into(), 1)],
            criteria: vec!["登录成功".into()],
            ..Default::default()
        });
        let verif = Arc::new(FakeVerif {
            found: Some(("req1".into(), 1, "v1".into())),
            ..Default::default()
        });
        let orch = DeliveryFeedbackOrchestrator::new(task, verif.clone(), Arc::new(AcceptAllJudge));

        orch.on_progress("d1", "t1", DeliveryProgress::Delivered { deliverable: dv("b", "d") })
            .await
            .expect("ok");
        let linked = verif.linked.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0], ("v1/t1".to_string(), vec!["登录成功".to_string()]));
        assert!(verif.synced.lock().unwrap_or_else(std::sync::PoisonError::into_inner)[0].1);
    }

    #[tokio::test]
    async fn delivered_failing_gate_fails_task_and_syncs_false() {
        let task =
            Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], ..Default::default() });
        let verif = Arc::new(FakeVerif {
            found: Some(("req1".into(), 1, "v1".into())),
            ..Default::default()
        });
        let orch =
            DeliveryFeedbackOrchestrator::new(task.clone(), verif.clone(), Arc::new(RuleJudge));

        let out = orch
            .on_progress(
                "d1",
                "t1",
                DeliveryProgress::Delivered { deliverable: dv("branch:x", "") },
            )
            .await
            .expect("ok");
        assert!(!out.verdict.as_ref().unwrap().passed);
        assert_eq!(
            task.advanced
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .last()
                .unwrap()
                .1,
            TaskTarget::Failed
        );
        assert!(!verif.synced.lock().unwrap_or_else(std::sync::PoisonError::into_inner)[0].1);
    }

    #[tokio::test]
    async fn accept_all_keeps_legacy_behavior() {
        let task =
            Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], ..Default::default() });
        let verif = Arc::new(FakeVerif {
            found: Some(("req1".into(), 1, "v1".into())),
            ..Default::default()
        });
        let orch = DeliveryFeedbackOrchestrator::new(task.clone(), verif, Arc::new(AcceptAllJudge));
        let out = orch
            .on_progress("d1", "t1", DeliveryProgress::Delivered { deliverable: dv("", "") })
            .await
            .expect("ok");
        assert!(out.verdict.unwrap().passed);
        assert_eq!(
            task.advanced
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .last()
                .unwrap()
                .1,
            TaskTarget::Verified
        );
    }

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
            let mut n = self.calls.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            *n += 1;
            if *n >= self.fix_after {
                Ok(dv("branch:fixed", "已据反馈补齐"))
            } else {
                Ok(dv("branch:retry", ""))
            }
        }
    }

    #[tokio::test]
    async fn revision_loop_fixes_and_verifies() {
        let task =
            Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], ..Default::default() });
        let verif = Arc::new(FakeVerif {
            found: Some(("req1".into(), 1, "v1".into())),
            ..Default::default()
        });
        let reviser = Arc::new(FakeReviser { calls: Mutex::new(0), fix_after: 1 });
        let orch =
            DeliveryFeedbackOrchestrator::new(task.clone(), verif.clone(), Arc::new(RuleJudge))
                .with_revision(reviser, 3);
        let out = orch
            .on_progress(
                "d1",
                "t1",
                DeliveryProgress::Delivered { deliverable: dv("branch:x", "") },
            )
            .await
            .expect("ok");
        assert!(out.verdict.as_ref().unwrap().passed);
        assert_eq!(out.revisions, 1);
        assert_eq!(
            task.advanced
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .last()
                .unwrap()
                .1,
            TaskTarget::Verified
        );
        assert!(
            verif
                .synced
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .last()
                .unwrap()
                .1
        );
    }

    #[tokio::test]
    async fn revision_loop_exhausts_then_fails() {
        let task =
            Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], ..Default::default() });
        let verif = Arc::new(FakeVerif {
            found: Some(("req1".into(), 1, "v1".into())),
            ..Default::default()
        });
        let reviser = Arc::new(FakeReviser { calls: Mutex::new(0), fix_after: 99 });
        let orch =
            DeliveryFeedbackOrchestrator::new(task.clone(), verif.clone(), Arc::new(RuleJudge))
                .with_revision(reviser, 2);
        let out = orch
            .on_progress(
                "d1",
                "t1",
                DeliveryProgress::Delivered { deliverable: dv("branch:x", "") },
            )
            .await
            .expect("ok");
        assert!(!out.verdict.as_ref().unwrap().passed);
        assert_eq!(out.revisions, 2);
        assert_eq!(
            task.advanced
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .last()
                .unwrap()
                .1,
            TaskTarget::Failed
        );
    }

    #[tokio::test]
    async fn running_advances_only() {
        let task =
            Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], ..Default::default() });
        let orch = DeliveryFeedbackOrchestrator::new(
            task.clone(),
            Arc::new(FakeVerif::default()),
            Arc::new(AcceptAllJudge),
        );
        let out = orch.on_progress("d1", "t1", DeliveryProgress::Running).await.expect("ok");
        assert_eq!(out.verification, VerificationSync::NotApplicable);
        assert!(out.verdict.is_none());
        assert_eq!(
            task.advanced.lock().unwrap_or_else(std::sync::PoisonError::into_inner)[0].1,
            TaskTarget::Running
        );
    }
}
