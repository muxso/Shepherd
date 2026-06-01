//! 内置 judge:`AcceptAllJudge`(默认,保持"交付即通过")与 `RuleJudge`(规则门:
//! 交付物须有非空 reference + summary,否则判不通过)。LLM judge 由组装根以 HTTP 适配器接入。

use async_trait::async_trait;

use crate::ports::{DeliverableView, Judge, Verdict};

/// 一律通过(默认,等价于"交付即 Verified")。
pub struct AcceptAllJudge;

#[async_trait]
impl Judge for AcceptAllJudge {
    async fn judge(&self, _criteria: &[String], _deliverable: &DeliverableView) -> Verdict {
        Verdict { passed: true, reason: "accept-all".into() }
    }
}

/// 规则门:要求交付物有非空 reference 与 summary(证明执行者确有产出)。
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
