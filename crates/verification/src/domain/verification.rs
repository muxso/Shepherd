//! 完整性验证领域模型:覆盖链聚合 → 每条标准状态 → 整体完整性报告。

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

/// 一条标准当前的覆盖状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriterionStatus {
    /// 无任何任务覆盖(前向缺口)。
    Uncovered,
    /// 有任务覆盖,但无一已交付+验证(后向缺口)。
    Pending,
    /// 至少一个覆盖任务已交付+验证。
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

/// 缺口类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    /// 需求→任务:这条标准没有任何任务覆盖。
    Uncovered,
    /// 任务→实现:覆盖了但覆盖任务尚未交付+验证。
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

/// 覆盖链:某任务覆盖某条标准。`satisfied` = 该任务已交付且验证。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageLink {
    pub decomposition_id: String,
    pub task_id: String,
    pub satisfied: bool,
}

/// 一条被验证的验收标准 + 它的覆盖链。
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

/// 一处缺口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    pub criterion_index: u32,
    pub text: String,
    pub kind: GapKind,
}

/// 单条标准在报告中的呈现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionReport {
    pub index: u32,
    pub text: String,
    pub status: CriterionStatus,
    pub links: Vec<CoverageLink>,
}

/// 完整性报告:整体是否完成 + 每条标准状态 + 缺口清单。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletenessReport {
    pub complete: bool,
    pub total: usize,
    pub satisfied: usize,
    pub criteria: Vec<CriterionReport>,
    pub gaps: Vec<Gap>,
}

/// 建立验证的入站请求(尚无 id)。构造即校验:requirement 非空、每条标准非空(trim)。
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

/// 一个需求版本的完整性验证聚合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    pub id: String,
    pub requirement_id: String,
    pub requirement_version: u32,
    pub criteria: Vec<VerifiedCriterion>,
}

impl Verification {
    /// 由已校验的 `NewVerification` + id 建立聚合(标准索引按位置 0-based,覆盖链初始为空)。
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

    /// 便捷构造(校验 + 建聚合),主要供领域测试使用。
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

    /// 建立覆盖链:任务覆盖某条标准(初始 satisfied=false;重复则忽略)。
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

    /// 同步某任务的交付/验证状态到它的**所有**覆盖链(delivery 验证通过即 satisfied=true)。
    /// 返回被更新的链数。
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

    /// 完整性报告:聚合每条标准状态,列出缺口;
    /// `complete` = 标准非空且**全部** Satisfied。
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
        // 后向缺口:覆盖未验证
        let r = v.report();
        assert_eq!(r.gaps.iter().filter(|g| g.kind == GapKind::Unverified).count(), 1);

        // 任务验证通过 → satisfied
        assert_eq!(v.sync_task("d1", "t1", true), 1);
        assert_eq!(v.criterion(0).expect("c0").status(), CriterionStatus::Satisfied);
    }

    #[test]
    fn complete_only_when_all_criteria_satisfied() {
        let mut v = v();
        v.link(0, "d1", "t1").expect("l0");
        v.link(1, "d1", "t2").expect("l1");
        v.sync_task("d1", "t1", true);
        // 标准1仍未验证
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
        // 一条标准被两个任务覆盖,任一验证即满足
        let mut v = v();
        v.link(0, "d1", "t1").expect("l");
        v.link(0, "d1", "t2").expect("l");
        v.sync_task("d1", "t2", true);
        assert_eq!(v.criterion(0).expect("c").status(), CriterionStatus::Satisfied);
    }

    #[test]
    fn sync_task_touches_all_its_links_across_criteria() {
        // 一个任务覆盖多条标准
        let mut v = v();
        v.link(0, "d1", "t1").expect("l");
        v.link(1, "d1", "t1").expect("l");
        assert_eq!(v.sync_task("d1", "t1", true), 2);
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
