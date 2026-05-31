//! 用例:交付结果 → 验证回灌。
//!
//! 一次交付尝试进入终态后调用 `on_settled`:据拆分图定位需求版本 → 找到验证 → 同步该任务的
//! 覆盖链 `satisfied`(Delivered ⇒ true,Failed ⇒ false)。验证报告随之刷新完整性/缺口。

use std::sync::Arc;

use crate::ports::{DecompositionGateway, OrchError, VerificationGateway};

/// 回灌结果(便于观测与测试)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedbackOutcome {
    /// 找不到拆分图,无从定位需求 —— 跳过。
    NoDecomposition,
    /// 该需求版本尚未开启验证 —— 跳过。
    NoVerification,
    /// 已把该任务状态同步进验证。
    Synced { verification_id: String, satisfied: bool },
}

#[derive(Clone)]
pub struct DeliveryFeedbackOrchestrator {
    decomposition: Arc<dyn DecompositionGateway>,
    verification: Arc<dyn VerificationGateway>,
}

impl DeliveryFeedbackOrchestrator {
    pub fn new(
        decomposition: Arc<dyn DecompositionGateway>,
        verification: Arc<dyn VerificationGateway>,
    ) -> Self {
        Self { decomposition, verification }
    }

    /// 交付尝试进入终态后回灌验证。`delivered` = 该尝试是否交付成功(Delivered)。
    pub async fn on_settled(
        &self,
        decomposition_id: &str,
        task_id: &str,
        delivered: bool,
    ) -> Result<FeedbackOutcome, OrchError> {
        let Some((requirement_id, version)) =
            self.decomposition.requirement_of(decomposition_id).await?
        else {
            return Ok(FeedbackOutcome::NoDecomposition);
        };
        let Some(verification_id) =
            self.verification.find_verification(&requirement_id, version).await?
        else {
            return Ok(FeedbackOutcome::NoVerification);
        };
        self.verification.sync(&verification_id, decomposition_id, task_id, delivered).await?;
        Ok(FeedbackOutcome::Synced { verification_id, satisfied: delivered })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeDecomp {
        // decomposition_id -> (requirement_id, version)
        map: Mutex<Vec<(String, String, u32)>>,
    }
    impl FakeDecomp {
        fn with(id: &str, req: &str, ver: u32) -> Self {
            Self { map: Mutex::new(vec![(id.into(), req.into(), ver)]) }
        }
    }
    #[async_trait]
    impl DecompositionGateway for FakeDecomp {
        async fn requirement_of(&self, id: &str) -> Result<Option<(String, u32)>, OrchError> {
            Ok(self
                .map
                .lock()
                .unwrap()
                .iter()
                .find(|(d, _, _)| d == id)
                .map(|(_, r, v)| (r.clone(), *v)))
        }
    }

    #[derive(Default)]
    struct FakeVerif {
        // (requirement_id, version) -> verification_id
        found: Option<(String, u32, String)>,
        // 记录 sync 调用
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
    async fn delivered_syncs_verification_satisfied_true() {
        let decomp = Arc::new(FakeDecomp::with("d1", "req1", 1));
        let verif = Arc::new(FakeVerif {
            found: Some(("req1".into(), 1, "v1".into())),
            ..Default::default()
        });
        let orch = DeliveryFeedbackOrchestrator::new(decomp, verif.clone());

        let out = orch.on_settled("d1", "t1", true).await.expect("ok");
        assert_eq!(out, FeedbackOutcome::Synced { verification_id: "v1".into(), satisfied: true });
        assert_eq!(verif.synced.lock().unwrap().as_slice(), &[("v1".into(), "d1".into(), "t1".into(), true)]);
    }

    #[tokio::test]
    async fn failed_syncs_satisfied_false() {
        let decomp = Arc::new(FakeDecomp::with("d1", "req1", 1));
        let verif = Arc::new(FakeVerif { found: Some(("req1".into(), 1, "v1".into())), ..Default::default() });
        let orch = DeliveryFeedbackOrchestrator::new(decomp, verif.clone());

        let out = orch.on_settled("d1", "t1", false).await.expect("ok");
        assert_eq!(out, FeedbackOutcome::Synced { verification_id: "v1".into(), satisfied: false });
        assert!(!verif.synced.lock().unwrap()[0].3);
    }

    #[tokio::test]
    async fn no_decomposition_is_skipped() {
        let orch = DeliveryFeedbackOrchestrator::new(
            Arc::new(FakeDecomp::default()),
            Arc::new(FakeVerif::default()),
        );
        assert_eq!(orch.on_settled("ghost", "t1", true).await.expect("ok"), FeedbackOutcome::NoDecomposition);
    }

    #[tokio::test]
    async fn no_verification_is_skipped() {
        let orch = DeliveryFeedbackOrchestrator::new(
            Arc::new(FakeDecomp::with("d1", "req1", 1)),
            Arc::new(FakeVerif::default()), // found = None
        );
        assert_eq!(orch.on_settled("d1", "t1", true).await.expect("ok"), FeedbackOutcome::NoVerification);
    }
}
