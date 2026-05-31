//! 用例:交付进度 → 同时(1)驱动任务生命周期 +(2)终态时回灌验证。
//!
//! 一次交付尝试每进入 Running/Delivered/Failed,编排器据进度:
//! - **镜像任务状态**:Running⇒任务推进到 Running,Delivered⇒Delivered,Failed⇒Failed(尽力而为);
//! - **回灌验证**(仅终态):据拆分图定位需求版本 → 找验证 → 同步该任务覆盖链
//!   `satisfied`(Delivered⇒true,Failed⇒false)。

use std::sync::Arc;

use crate::ports::{OrchError, TaskGateway, TaskTarget, VerificationGateway};

/// 交付进度(对应 delivery 尝试的非初始状态)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryProgress {
    Running,
    Delivered,
    Failed,
}

/// 验证回灌结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationSync {
    /// 非终态(Running),暂不回灌。
    NotApplicable,
    /// 找不到拆分图,无从定位需求。
    NoDecomposition,
    /// 该需求版本尚未开启验证。
    NoVerification,
    /// 已同步覆盖链。
    Synced { verification_id: String, satisfied: bool },
}

/// 一次编排的效果(便于观测与测试)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackOutcome {
    /// 任务是否被成功推进(尽力而为:依赖未满足等会导致 false)。
    pub task_advanced: bool,
    pub verification: VerificationSync,
}

#[derive(Clone)]
pub struct DeliveryFeedbackOrchestrator {
    task: Arc<dyn TaskGateway>,
    verification: Arc<dyn VerificationGateway>,
}

impl DeliveryFeedbackOrchestrator {
    pub fn new(task: Arc<dyn TaskGateway>, verification: Arc<dyn VerificationGateway>) -> Self {
        Self { task, verification }
    }

    /// 交付进度推进时调用:驱动任务 + (终态)回灌验证。
    pub async fn on_progress(
        &self,
        decomposition_id: &str,
        task_id: &str,
        progress: DeliveryProgress,
    ) -> Result<FeedbackOutcome, OrchError> {
        // (1) 驱动任务生命周期 —— 尽力而为(依赖未满足/非法流转不阻断验证回灌)。
        let target = match progress {
            DeliveryProgress::Running => TaskTarget::Running,
            DeliveryProgress::Delivered => TaskTarget::Delivered,
            DeliveryProgress::Failed => TaskTarget::Failed,
        };
        let task_advanced =
            self.task.advance_task(decomposition_id, task_id, target).await.is_ok();

        // (2) 终态回灌验证。
        let verification = match progress {
            DeliveryProgress::Running => VerificationSync::NotApplicable,
            DeliveryProgress::Delivered | DeliveryProgress::Failed => {
                let satisfied = matches!(progress, DeliveryProgress::Delivered);
                match self.task.requirement_of(decomposition_id).await? {
                    None => VerificationSync::NoDecomposition,
                    Some((req_id, version)) => {
                        match self.verification.find_verification(&req_id, version).await? {
                            None => VerificationSync::NoVerification,
                            Some(vid) => {
                                self.verification
                                    .sync(&vid, decomposition_id, task_id, satisfied)
                                    .await?;
                                VerificationSync::Synced { verification_id: vid, satisfied }
                            }
                        }
                    }
                }
            }
        };

        Ok(FeedbackOutcome { task_advanced, verification })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeTask {
        // decomposition_id -> (requirement_id, version)
        map: Vec<(String, String, u32)>,
        advanced: Mutex<Vec<(String, String, TaskTarget)>>,
        // 模拟推进失败(如依赖未满足)
        advance_fails: bool,
    }
    impl FakeTask {
        fn with(id: &str, req: &str, ver: u32) -> Self {
            Self { map: vec![(id.into(), req.into(), ver)], ..Default::default() }
        }
    }
    #[async_trait]
    impl TaskGateway for FakeTask {
        async fn requirement_of(&self, id: &str) -> Result<Option<(String, u32)>, OrchError> {
            Ok(self.map.iter().find(|(d, _, _)| d == id).map(|(_, r, v)| (r.clone(), *v)))
        }
        async fn advance_task(&self, d: &str, t: &str, target: TaskTarget) -> Result<(), OrchError> {
            if self.advance_fails {
                return Err(OrchError::Gateway("deps not satisfied".into()));
            }
            self.advanced.lock().unwrap().push((d.into(), t.into(), target));
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeVerif {
        found: Option<(String, u32, String)>,
        synced: Mutex<Vec<(String, String, String, bool)>>,
    }
    #[async_trait]
    impl VerificationGateway for FakeVerif {
        async fn find_verification(&self, req: &str, ver: u32) -> Result<Option<String>, OrchError> {
            Ok(self.found.as_ref().filter(|(r, v, _)| r == req && *v == ver).map(|(_, _, id)| id.clone()))
        }
        async fn sync(&self, vid: &str, d: &str, t: &str, s: bool) -> Result<(), OrchError> {
            self.synced.lock().unwrap().push((vid.into(), d.into(), t.into(), s));
            Ok(())
        }
    }

    #[tokio::test]
    async fn delivered_advances_task_and_syncs_verification() {
        let task = Arc::new(FakeTask::with("d1", "req1", 1));
        let verif = Arc::new(FakeVerif { found: Some(("req1".into(), 1, "v1".into())), ..Default::default() });
        let orch = DeliveryFeedbackOrchestrator::new(task.clone(), verif.clone());

        let out = orch.on_progress("d1", "t1", DeliveryProgress::Delivered).await.expect("ok");
        assert!(out.task_advanced);
        assert_eq!(out.verification, VerificationSync::Synced { verification_id: "v1".into(), satisfied: true });
        assert_eq!(task.advanced.lock().unwrap().as_slice(), &[("d1".into(), "t1".into(), TaskTarget::Delivered)]);
        assert!(verif.synced.lock().unwrap()[0].3);
    }

    #[tokio::test]
    async fn running_advances_task_only_no_verification() {
        let task = Arc::new(FakeTask::with("d1", "req1", 1));
        let verif = Arc::new(FakeVerif { found: Some(("req1".into(), 1, "v1".into())), ..Default::default() });
        let orch = DeliveryFeedbackOrchestrator::new(task.clone(), verif.clone());

        let out = orch.on_progress("d1", "t1", DeliveryProgress::Running).await.expect("ok");
        assert!(out.task_advanced);
        assert_eq!(out.verification, VerificationSync::NotApplicable);
        assert_eq!(task.advanced.lock().unwrap()[0].2, TaskTarget::Running);
        assert!(verif.synced.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_advances_failed_and_syncs_unsatisfied() {
        let task = Arc::new(FakeTask::with("d1", "req1", 1));
        let verif = Arc::new(FakeVerif { found: Some(("req1".into(), 1, "v1".into())), ..Default::default() });
        let orch = DeliveryFeedbackOrchestrator::new(task.clone(), verif.clone());

        let out = orch.on_progress("d1", "t1", DeliveryProgress::Failed).await.expect("ok");
        assert_eq!(out.verification, VerificationSync::Synced { verification_id: "v1".into(), satisfied: false });
        assert_eq!(task.advanced.lock().unwrap()[0].2, TaskTarget::Failed);
    }

    #[tokio::test]
    async fn task_advance_failure_does_not_block_verification() {
        let task = Arc::new(FakeTask { map: vec![("d1".into(), "req1".into(), 1)], advance_fails: true, ..Default::default() });
        let verif = Arc::new(FakeVerif { found: Some(("req1".into(), 1, "v1".into())), ..Default::default() });
        let orch = DeliveryFeedbackOrchestrator::new(task, verif.clone());

        let out = orch.on_progress("d1", "t1", DeliveryProgress::Delivered).await.expect("ok");
        assert!(!out.task_advanced); // 推进失败
        // 但验证照常回灌
        assert!(matches!(out.verification, VerificationSync::Synced { .. }));
    }

    #[tokio::test]
    async fn no_verification_still_advances_task() {
        let task = Arc::new(FakeTask::with("d1", "req1", 1));
        let orch = DeliveryFeedbackOrchestrator::new(task.clone(), Arc::new(FakeVerif::default()));
        let out = orch.on_progress("d1", "t1", DeliveryProgress::Delivered).await.expect("ok");
        assert!(out.task_advanced);
        assert_eq!(out.verification, VerificationSync::NoVerification);
    }
}
