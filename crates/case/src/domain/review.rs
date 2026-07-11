use std::collections::BTreeMap;

pub const SYSTEM_USER: &str = "system";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewStatus {
    UnReviewed,
    UnderReviewed,
    Pass,
    UnPass,
    ReReviewed,
}

impl ReviewStatus {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassRule {
    Single,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewSetting {
    pub rule: PassRule,
    pub reviewer_count: usize,
}

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ReviewError {
    #[error("content is required when marking UN_PASS")]
    ContentRequiredForUnPass,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRecord {
    pub reviewer_id: String,
    pub status: ReviewStatus,
}

// `history` must be time-ascending (old→new): a reviewer's later record overrides earlier ones.
pub fn effective_verdicts(history: &[ReviewRecord]) -> BTreeMap<String, ReviewStatus> {
    let mut latest = BTreeMap::new();
    for r in history {
        if r.status == ReviewStatus::UnderReviewed || r.reviewer_id == SYSTEM_USER {
            continue;
        }
        latest.insert(r.reviewer_id.clone(), r.status);
    }
    latest
}

pub fn aggregate_status(setting: ReviewSetting, history: &[ReviewRecord]) -> ReviewStatus {
    match setting.rule {
        PassRule::Single => history
            .iter()
            .rev()
            .find(|r| r.status != ReviewStatus::UnderReviewed && r.reviewer_id != SYSTEM_USER)
            .map(|r| r.status)
            .unwrap_or(ReviewStatus::UnReviewed),

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

pub fn review_completed(statuses: &[ReviewStatus]) -> bool {
    !statuses.is_empty()
        && statuses.iter().all(|s| matches!(s, ReviewStatus::Pass | ReviewStatus::UnPass))
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

    #[test]
    fn status_string_roundtrip() {
        for s in [UnReviewed, UnderReviewed, Pass, UnPass, ReReviewed] {
            assert_eq!(ReviewStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(ReviewStatus::parse("pass"), Some(Pass));
        assert_eq!(ReviewStatus::parse("garbage"), None);
    }

    #[test]
    fn pass_rule_string_roundtrip() {
        assert_eq!(PassRule::parse(PassRule::Single.as_str()), Some(PassRule::Single));
        assert_eq!(PassRule::parse("multiple"), Some(PassRule::Multiple));
        assert_eq!(PassRule::parse("x"), None);
    }

    #[test]
    fn un_pass_requires_content() {
        assert_eq!(Verdict::new("u1", UnPass, None), Err(ReviewError::ContentRequiredForUnPass));
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

    #[test]
    fn latest_verdict_per_reviewer_wins() {
        let h = [rec("u1", UnPass), rec("u1", Pass)];
        let eff = effective_verdicts(&h);
        assert_eq!(eff.get("u1"), Some(&Pass));
    }

    #[test]
    fn suggestion_does_not_count_as_verdict() {
        let h = [rec("u1", Pass), rec("u1", UnderReviewed)];
        assert_eq!(effective_verdicts(&h).get("u1"), Some(&Pass));
    }

    #[test]
    fn system_user_is_excluded() {
        let h = [rec(SYSTEM_USER, Pass), rec("u1", Pass)];
        let eff = effective_verdicts(&h);
        assert!(!eff.contains_key(SYSTEM_USER));
        assert_eq!(eff.len(), 1);
    }

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

    #[test]
    fn multiple_any_unpass_fails_whole() {
        let h = [rec("u1", Pass), rec("u2", UnPass)];
        assert_eq!(aggregate_status(multiple(2), &h), UnPass);
    }

    #[test]
    fn multiple_partial_pass_is_under_review() {
        let h = [rec("u1", Pass)];
        assert_eq!(aggregate_status(multiple(2), &h), UnderReviewed);
    }

    #[test]
    fn multiple_all_pass_is_pass() {
        let h = [rec("u1", Pass), rec("u2", Pass)];
        assert_eq!(aggregate_status(multiple(2), &h), Pass);
    }

    #[test]
    fn multiple_no_verdicts_is_un_reviewed() {
        let h = [rec("u1", UnderReviewed)];
        assert_eq!(aggregate_status(multiple(2), &h), UnReviewed);
    }

    #[test]
    fn multiple_reviewer_changes_unpass_to_pass() {
        let h = [rec("u1", Pass), rec("u2", UnPass), rec("u2", Pass)];
        assert_eq!(aggregate_status(multiple(2), &h), Pass);
    }

    #[test]
    fn multiple_system_vote_not_counted_toward_total() {
        let h = [rec(SYSTEM_USER, Pass), rec("u1", Pass)];
        assert_eq!(aggregate_status(multiple(2), &h), UnderReviewed);
    }

    #[test]
    fn empty_review_is_not_completed() {
        assert!(!review_completed(&[]));
    }

    #[test]
    fn all_terminal_verdicts_complete_the_review() {
        assert!(review_completed(&[Pass, Pass]));
        assert!(review_completed(&[Pass, UnPass]));
        assert!(review_completed(&[UnPass]));
    }

    #[test]
    fn pending_case_blocks_completion() {
        assert!(!review_completed(&[Pass, UnReviewed]));
        assert!(!review_completed(&[Pass, UnderReviewed]));
        assert!(!review_completed(&[ReReviewed, Pass]));
    }
}
