use async_trait::async_trait;

use crate::ports::{DeliverableView, Judge, Verdict};

pub struct AcceptAllJudge;

#[async_trait]
impl Judge for AcceptAllJudge {
    async fn judge(&self, _criteria: &[String], _deliverable: &DeliverableView) -> Verdict {
        Verdict { passed: true, reason: "accept-all".into() }
    }
}

pub struct RuleJudge;

#[async_trait]
impl Judge for RuleJudge {
    async fn judge(&self, _criteria: &[String], deliverable: &DeliverableView) -> Verdict {
        if deliverable.reference.trim().is_empty() {
            Verdict { passed: false, reason: "交付物缺少 reference".into() }
        } else if deliverable.summary.trim().is_empty() {
            Verdict { passed: false, reason: "交付物缺少 summary".into() }
        } else {
            Verdict { passed: true, reason: "交付物完整".into() }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dv(reference: &str, summary: &str) -> DeliverableView {
        DeliverableView { kind: "DIFF".into(), reference: reference.into(), summary: summary.into() }
    }

    #[tokio::test]
    async fn accept_all_passes() {
        assert!(AcceptAllJudge.judge(&[], &dv("", "")).await.passed);
    }

    #[tokio::test]
    async fn rule_judge_gates_empty_deliverable() {
        assert!(RuleJudge.judge(&[], &dv("branch:x", "done")).await.passed);
        assert!(!RuleJudge.judge(&[], &dv("", "done")).await.passed);
        assert!(!RuleJudge.judge(&[], &dv("branch:x", "  ")).await.passed);
    }
}
