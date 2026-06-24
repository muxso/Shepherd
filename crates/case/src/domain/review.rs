//! 用例评审状态机 + 会签/或签聚合。纯逻辑,无 IO,无时间依赖。
//!
//! 关键设计:历史记录按时间顺序(旧→新)给入,"最新结论"= 在序列中靠后的那条。
//! 以此**消除对时间戳的依赖**,聚合彻底确定性,测试可穷举。

use std::collections::BTreeMap;

/// SYSTEM 用户(自动重新提审会引入),不计入评审票数。
pub const SYSTEM_USER: &str = "system";

/// 单用例评审状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewStatus {
    UnReviewed,
    /// 评审中 / "建议":不构成最终结论,聚合时被排除。
    UnderReviewed,
    Pass,
    UnPass,
    ReReviewed,
}

impl ReviewStatus {
    /// 规范字符串(与 Java/DB 取值一致),供 PG 与 HTTP 适配器共用。
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewStatus::UnReviewed => "UN_REVIEWED",
            ReviewStatus::UnderReviewed => "UNDER_REVIEWED",
            ReviewStatus::Pass => "PASS",
            ReviewStatus::UnPass => "UN_PASS",
            ReviewStatus::ReReviewed => "RE_REVIEWED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "UN_REVIEWED" => Some(ReviewStatus::UnReviewed),
            "UNDER_REVIEWED" => Some(ReviewStatus::UnderReviewed),
            "PASS" => Some(ReviewStatus::Pass),
            "UN_PASS" => Some(ReviewStatus::UnPass),
            "RE_REVIEWED" => Some(ReviewStatus::ReReviewed),
            _ => None,
        }
    }
}

/// 评审通过规则。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassRule {
    /// 或签:任一评审人的最新结论即决定整体。
    Single,
    /// 会签:需所有评审人通过才算通过。
    Multiple,
}

impl PassRule {
    pub fn as_str(&self) -> &'static str {
        match self {
            PassRule::Single => "SINGLE",
            PassRule::Multiple => "MULTIPLE",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "SINGLE" => Some(PassRule::Single),
            "MULTIPLE" => Some(PassRule::Multiple),
            _ => None,
        }
    }
}

/// 评审配置:规则 + 评审人数(会签判定"全部通过"用)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewSetting {
    pub rule: PassRule,
    pub reviewer_count: usize,
}

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ReviewError {
    /// 提交 UnPass(不通过)必须填写评论内容。
    #[error("content is required when marking UN_PASS")]
    ContentRequiredForUnPass,
}

/// 一次评审动作(提交时校验)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub reviewer_id: String,
    pub status: ReviewStatus,
    pub content: Option<String>,
}

impl Verdict {
    pub fn new(
        reviewer_id: &str,
        status: ReviewStatus,
        content: Option<&str>,
    ) -> Result<Self, ReviewError> {
        let content = content.map(str::trim).filter(|c| !c.is_empty()).map(str::to_string);
        if status == ReviewStatus::UnPass && content.is_none() {
            return Err(ReviewError::ContentRequiredForUnPass);
        }
        Ok(Self { reviewer_id: reviewer_id.to_string(), status, content })
    }
}

/// 落库后的一条评审历史(按时间顺序排列)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRecord {
    pub reviewer_id: String,
    pub status: ReviewStatus,
}

/// 每个评审人「最新的非建议结论」,排除 SYSTEM 用户。
/// `history` 须按时间升序(旧→新);同一评审人后出现的覆盖先出现的。
pub fn effective_verdicts(history: &[ReviewRecord]) -> BTreeMap<String, ReviewStatus> {
    let mut latest = BTreeMap::new();
    for r in history {
        if r.status == ReviewStatus::UnderReviewed || r.reviewer_id == SYSTEM_USER {
            continue; // 建议不算结论;system 不计票
        }
        latest.insert(r.reviewer_id.clone(), r.status);
    }
    latest
}

/// 依据通过规则聚合出整体评审状态。
pub fn aggregate_status(setting: ReviewSetting, history: &[ReviewRecord]) -> ReviewStatus {
    match setting.rule {
        // 或签:最近一条「非建议、非 system」结论说了算。
        PassRule::Single => history
            .iter()
            .rev()
            .find(|r| r.status != ReviewStatus::UnderReviewed && r.reviewer_id != SYSTEM_USER)
            .map(|r| r.status)
            .unwrap_or(ReviewStatus::UnReviewed),

        // 会签:统计各评审人最新结论。
        PassRule::Multiple => {
            let effective = effective_verdicts(history);
            let pass = effective.values().filter(|s| **s == ReviewStatus::Pass).count();
            let un_pass = effective.values().filter(|s| **s == ReviewStatus::UnPass).count();

            if un_pass > 0 {
                ReviewStatus::UnPass
            } else if pass > 0 && pass < setting.reviewer_count {
                ReviewStatus::UnderReviewed
            } else if pass == setting.reviewer_count && setting.reviewer_count > 0 {
                ReviewStatus::Pass
            } else {
                ReviewStatus::UnReviewed
            }
        }
    }
}

/// 整个评审是否「已完成」:用例非空,且每条用例都已得出终态结论(PASS 或 UN_PASS)。
/// 只要还有 未评审 / 评审中 的用例,评审就未完成。终态判定与单条用例的聚合状态一致,
/// 因此「评审完成」可由各用例状态纯函数推导,无需额外时间或人工标记。
pub fn review_completed(statuses: &[ReviewStatus]) -> bool {
    !statuses.is_empty()
        && statuses
            .iter()
            .all(|s| matches!(s, ReviewStatus::Pass | ReviewStatus::UnPass))
}

#[cfg(test)]
mod tests {
    use super::ReviewStatus::*;
    use super::*;

    fn rec(reviewer: &str, status: ReviewStatus) -> ReviewRecord {
        ReviewRecord { reviewer_id: reviewer.to_string(), status }
    }
    fn single() -> ReviewSetting {
        ReviewSetting { rule: PassRule::Single, reviewer_count: 1 }
    }
    fn multiple(n: usize) -> ReviewSetting {
        ReviewSetting { rule: PassRule::Multiple, reviewer_count: n }
    }

    // ---- 字符串往返 ----
    #[test]
    fn status_string_roundtrip() {
        for s in [UnReviewed, UnderReviewed, Pass, UnPass, ReReviewed] {
            assert_eq!(ReviewStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(ReviewStatus::parse("pass"), Some(Pass)); // 大小写无关
        assert_eq!(ReviewStatus::parse("garbage"), None);
    }

    #[test]
    fn pass_rule_string_roundtrip() {
        assert_eq!(PassRule::parse(PassRule::Single.as_str()), Some(PassRule::Single));
        assert_eq!(PassRule::parse("multiple"), Some(PassRule::Multiple));
        assert_eq!(PassRule::parse("x"), None);
    }

    // ---- Verdict 校验 ----
    #[test]
    fn un_pass_requires_content() {
        assert_eq!(
            Verdict::new("u1", UnPass, None),
            Err(ReviewError::ContentRequiredForUnPass)
        );
        assert_eq!(
            Verdict::new("u1", UnPass, Some("   ")),
            Err(ReviewError::ContentRequiredForUnPass)
        );
    }

    #[test]
    fn un_pass_with_content_ok_and_pass_needs_no_content() {
        assert!(Verdict::new("u1", UnPass, Some("理由")).is_ok());
        assert!(Verdict::new("u1", Pass, None).is_ok());
    }

    // ---- effective_verdicts ----
    #[test]
    fn latest_verdict_per_reviewer_wins() {
        let h = [rec("u1", UnPass), rec("u1", Pass)]; // u1 改主意
        let eff = effective_verdicts(&h);
        assert_eq!(eff.get("u1"), Some(&Pass));
    }

    #[test]
    fn suggestion_does_not_count_as_verdict() {
        let h = [rec("u1", Pass), rec("u1", UnderReviewed)]; // 后补一条建议
        assert_eq!(effective_verdicts(&h).get("u1"), Some(&Pass)); // 仍是 Pass
    }

    #[test]
    fn system_user_is_excluded() {
        let h = [rec(SYSTEM_USER, Pass), rec("u1", Pass)];
        let eff = effective_verdicts(&h);
        assert!(!eff.contains_key(SYSTEM_USER));
        assert_eq!(eff.len(), 1);
    }

    // ---- 或签 SINGLE ----
    #[test]
    fn single_empty_is_un_reviewed() {
        assert_eq!(aggregate_status(single(), &[]), UnReviewed);
    }

    #[test]
    fn single_latest_decides() {
        let h = [rec("u1", Pass), rec("u2", UnPass)];
        assert_eq!(aggregate_status(single(), &h), UnPass);
    }

    #[test]
    fn single_suggestion_does_not_override_prior_pass() {
        let h = [rec("u1", Pass), rec("u2", UnderReviewed)];
        assert_eq!(aggregate_status(single(), &h), Pass);
    }

    // ---- 会签 MULTIPLE ----
    #[test]
    fn multiple_any_unpass_fails_whole() {
        let h = [rec("u1", Pass), rec("u2", UnPass)];
        assert_eq!(aggregate_status(multiple(2), &h), UnPass);
    }

    #[test]
    fn multiple_partial_pass_is_under_review() {
        let h = [rec("u1", Pass)]; // 2 人中 1 人通过
        assert_eq!(aggregate_status(multiple(2), &h), UnderReviewed);
    }

    #[test]
    fn multiple_all_pass_is_pass() {
        let h = [rec("u1", Pass), rec("u2", Pass)];
        assert_eq!(aggregate_status(multiple(2), &h), Pass);
    }

    #[test]
    fn multiple_no_verdicts_is_un_reviewed() {
        let h = [rec("u1", UnderReviewed)]; // 只有建议
        assert_eq!(aggregate_status(multiple(2), &h), UnReviewed);
    }

    #[test]
    fn multiple_reviewer_changes_unpass_to_pass() {
        // u2 先不通过(带理由)后改通过 → 不再计 unpass,2/2 通过
        let h = [rec("u1", Pass), rec("u2", UnPass), rec("u2", Pass)];
        assert_eq!(aggregate_status(multiple(2), &h), Pass);
    }

    #[test]
    fn multiple_system_vote_not_counted_toward_total() {
        // system 的 Pass 不计票:真实评审人 2 人只到 1 人 → 评审中
        let h = [rec(SYSTEM_USER, Pass), rec("u1", Pass)];
        assert_eq!(aggregate_status(multiple(2), &h), UnderReviewed);
    }

    // ---- 评审完成 ----
    #[test]
    fn empty_review_is_not_completed() {
        assert!(!review_completed(&[]));
    }

    #[test]
    fn all_terminal_verdicts_complete_the_review() {
        assert!(review_completed(&[Pass, Pass]));
        assert!(review_completed(&[Pass, UnPass])); // 含不通过也算「评审完成」
        assert!(review_completed(&[UnPass]));
    }

    #[test]
    fn pending_case_blocks_completion() {
        assert!(!review_completed(&[Pass, UnReviewed]));
        assert!(!review_completed(&[Pass, UnderReviewed]));
        assert!(!review_completed(&[ReReviewed, Pass])); // 重新评审 = 重新打开
    }
}
