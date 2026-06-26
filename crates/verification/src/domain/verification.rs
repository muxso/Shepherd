use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VerificationError {
    #[error("requirement id must not be empty")]
    EmptyRequirement,
    #[error("acceptance criterion must not be empty")]
    EmptyCriterion,
    #[error("no such criterion index: {0}")]
    NoSuchCriterion(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriterionStatus {
    Uncovered,
    Pending,
    Satisfied,
}

impl CriterionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Uncovered => "UNCOVERED",
            Self::Pending => "PENDING",
            Self::Satisfied => "SATISFIED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    Uncovered,
    Unverified,
}

impl GapKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Uncovered => "UNCOVERED",
            Self::Unverified => "UNVERIFIED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageLink {
    pub decomposition_id: String,
    pub task_id: String,
    pub satisfied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCriterion {
    pub index: u32,
    pub text: String,
    pub links: Vec<CoverageLink>,
}

impl VerifiedCriterion {
    pub fn status(&self) -> CriterionStatus {
        if self.links.is_empty() {
            CriterionStatus::Uncovered
        } else if self.links.iter().any(|l| l.satisfied) {
            CriterionStatus::Satisfied
        } else {
            CriterionStatus::Pending
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    pub criterion_index: u32,
    pub text: String,
    pub kind: GapKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionReport {
    pub index: u32,
    pub text: String,
    pub status: CriterionStatus,
    pub links: Vec<CoverageLink>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletenessReport {
    pub complete: bool,
    pub total: usize,
    pub satisfied: usize,
    pub criteria: Vec<CriterionReport>,
    pub gaps: Vec<Gap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewVerification {
    pub requirement_id: String,
    pub requirement_version: u32,
    pub criteria: Vec<String>,
}

impl NewVerification {
    pub fn new(
        requirement_id: &str,
        requirement_version: u32,
        criteria_texts: &[String],
    ) -> Result<Self, VerificationError> {
        let requirement_id = requirement_id.trim();
        if requirement_id.is_empty() {
            return Err(VerificationError::EmptyRequirement);
        }
        let mut criteria = Vec::with_capacity(criteria_texts.len());
        for text in criteria_texts {
            let text = text.trim();
            if text.is_empty() {
                return Err(VerificationError::EmptyCriterion);
            }
            criteria.push(text.to_string());
        }
        Ok(Self { requirement_id: requirement_id.to_string(), requirement_version, criteria })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    pub id: String,
    pub requirement_id: String,
    pub requirement_version: u32,
    pub criteria: Vec<VerifiedCriterion>,
}

impl Verification {
    pub fn from_new(id: &str, new: &NewVerification) -> Self {
        let criteria = new
            .criteria
            .iter()
            .enumerate()
            .map(|(i, text)| VerifiedCriterion { index: i as u32, text: text.clone(), links: Vec::new() })
            .collect();
        Self {
            id: id.to_string(),
            requirement_id: new.requirement_id.clone(),
            requirement_version: new.requirement_version,
            criteria,
        }
    }

    pub fn create(
        id: &str,
        requirement_id: &str,
        requirement_version: u32,
        criteria_texts: &[String],
    ) -> Result<Self, VerificationError> {
        Ok(Self::from_new(id, &NewVerification::new(requirement_id, requirement_version, criteria_texts)?))
    }

    pub fn criterion(&self, index: u32) -> Option<&VerifiedCriterion> {
        self.criteria.iter().find(|c| c.index == index)
    }

    pub fn link(
        &mut self,
        criterion_index: u32,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<(), VerificationError> {
        let c = self
            .criteria
            .iter_mut()
            .find(|c| c.index == criterion_index)
            .ok_or(VerificationError::NoSuchCriterion(criterion_index))?;
        let dup = c
            .links
            .iter()
            .any(|l| l.decomposition_id == decomposition_id && l.task_id == task_id);
        if !dup {
            c.links.push(CoverageLink {
                decomposition_id: decomposition_id.to_string(),
                task_id: task_id.to_string(),
                satisfied: false,
            });
        }
        Ok(())
    }

    pub fn link_task_by_texts(
        &mut self,
        decomposition_id: &str,
        task_id: &str,
        texts: &[String],
    ) -> usize {
        let indices: Vec<u32> = self
            .criteria
            .iter()
            .filter(|c| texts.iter().any(|t| t.trim() == c.text))
            .map(|c| c.index)
            .collect();
        let mut created = 0;
        for idx in indices {
            let before = self.criterion(idx).map(|c| c.links.len()).unwrap_or(0);
            // idx 来自自身标准,link 必成功;丢弃 Result 安全。
            let _ = self.link(idx, decomposition_id, task_id);
            let after = self.criterion(idx).map(|c| c.links.len()).unwrap_or(0);
            if after > before {
                created += 1;
            }
        }
        created
    }

    pub fn sync_task(&mut self, decomposition_id: &str, task_id: &str, satisfied: bool) -> usize {
        let mut n = 0;
        for c in &mut self.criteria {
            for l in &mut c.links {
                if l.decomposition_id == decomposition_id && l.task_id == task_id {
                    l.satisfied = satisfied;
                    n += 1;
                }
            }
        }
        n
    }

    pub fn report(&self) -> CompletenessReport {
        let mut criteria = Vec::with_capacity(self.criteria.len());
        let mut gaps = Vec::new();
        let mut satisfied = 0;
        for c in &self.criteria {
            let status = c.status();
            match status {
                CriterionStatus::Satisfied => satisfied += 1,
                CriterionStatus::Uncovered => gaps.push(Gap {
                    criterion_index: c.index,
                    text: c.text.clone(),
                    kind: GapKind::Uncovered,
                }),
                CriterionStatus::Pending => gaps.push(Gap {
                    criterion_index: c.index,
                    text: c.text.clone(),
                    kind: GapKind::Unverified,
                }),
            }
            criteria.push(CriterionReport {
                index: c.index,
                text: c.text.clone(),
                status,
                links: c.links.clone(),
            });
        }
        CompletenessReport {
            complete: !self.criteria.is_empty() && gaps.is_empty(),
            total: self.criteria.len(),
            satisfied,
            criteria,
            gaps,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.report().complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v() -> Verification {
        Verification::create("v1", "req1", 1, &["登录成功".into(), "错误密码拒绝".into()])
            .expect("valid")
    }

    #[test]
    fn create_validates() {
        assert_eq!(
            Verification::create("v1", " ", 1, &[]).unwrap_err(),
            VerificationError::EmptyRequirement
        );
        assert_eq!(
            Verification::create("v1", "r", 1, &["ok".into(), "  ".into()]).unwrap_err(),
            VerificationError::EmptyCriterion
        );
        let v = v();
        assert_eq!(v.criteria.len(), 2);
        assert_eq!(v.criterion(0).expect("c0").index, 0);
    }

    #[test]
    fn fresh_criteria_are_uncovered_gaps() {
        let r = v().report();
        assert!(!r.complete);
        assert_eq!(r.total, 2);
        assert_eq!(r.satisfied, 0);
        assert_eq!(r.gaps.len(), 2);
        assert!(r.gaps.iter().all(|g| g.kind == GapKind::Uncovered));
    }

    #[test]
    fn link_makes_pending_then_sync_makes_satisfied() {
        let mut v = v();
        v.link(0, "d1", "t1").expect("link");
        assert_eq!(v.criterion(0).expect("c0").status(), CriterionStatus::Pending);
        let r = v.report();
        assert_eq!(r.gaps.iter().filter(|g| g.kind == GapKind::Unverified).count(), 1);

        assert_eq!(v.sync_task("d1", "t1", true), 1);
        assert_eq!(v.criterion(0).expect("c0").status(), CriterionStatus::Satisfied);
    }

    #[test]
    fn complete_only_when_all_criteria_satisfied() {
        let mut v = v();
        v.link(0, "d1", "t1").expect("l0");
        v.link(1, "d1", "t2").expect("l1");
        v.sync_task("d1", "t1", true);
        let r = v.report();
        assert!(!r.complete);
        assert_eq!(r.satisfied, 1);
        assert_eq!(r.gaps.len(), 1);
        assert_eq!(r.gaps[0].criterion_index, 1);
        assert_eq!(r.gaps[0].kind, GapKind::Unverified);

        v.sync_task("d1", "t2", true);
        let r = v.report();
        assert!(r.complete);
        assert_eq!(r.satisfied, 2);
        assert!(r.gaps.is_empty());
    }

    #[test]
    fn one_satisfied_link_suffices_for_criterion() {
        let mut v = v();
        v.link(0, "d1", "t1").expect("l");
        v.link(0, "d1", "t2").expect("l");
        v.sync_task("d1", "t2", true);
        assert_eq!(v.criterion(0).expect("c").status(), CriterionStatus::Satisfied);
    }

    #[test]
    fn sync_task_touches_all_its_links_across_criteria() {
        let mut v = v();
        v.link(0, "d1", "t1").expect("l");
        v.link(1, "d1", "t1").expect("l");
        assert_eq!(v.sync_task("d1", "t1", true), 2);
        assert!(v.is_complete());
    }

    #[test]
    fn link_task_by_texts_matches_and_dedups() {
        let mut v = v();
        let n = v.link_task_by_texts("d1", "t1", &[" 登录成功 ".into(), "无关标准".into()]);
        assert_eq!(n, 1);
        assert_eq!(v.criterion(0).expect("c0").status(), CriterionStatus::Pending);
        assert_eq!(v.criterion(1).expect("c1").status(), CriterionStatus::Uncovered);
        assert_eq!(v.link_task_by_texts("d1", "t1", &["登录成功".into()]), 0);
    }

    #[test]
    fn link_task_by_texts_integration_task_covers_all() {
        let mut v = v();
        let n = v.link_task_by_texts("d1", "t9", &["登录成功".into(), "错误密码拒绝".into()]);
        assert_eq!(n, 2);
        v.sync_task("d1", "t9", true);
        assert!(v.is_complete());
    }

    #[test]
    fn link_dedup_and_unknown_index() {
        let mut v = v();
        v.link(0, "d1", "t1").expect("l");
        v.link(0, "d1", "t1").expect("dup ignored");
        assert_eq!(v.criterion(0).expect("c").links.len(), 1);
        assert_eq!(v.link(9, "d1", "t1").unwrap_err(), VerificationError::NoSuchCriterion(9));
    }
}
