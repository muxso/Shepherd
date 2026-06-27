use thiserror::Error;

/// 与 DB 列一致。
pub const MAX_TITLE_LEN: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RequirementError {
    #[error("requirement title must not be empty")]
    EmptyTitle,
    #[error("requirement title too long")]
    TitleTooLong,
    #[error("project id must not be empty")]
    EmptyProject,
    #[error("acceptance criterion must not be empty")]
    EmptyCriterion,
    #[error("no such version: {0}")]
    NoSuchVersion(u32),
    #[error("cannot revise an archived requirement")]
    Archived,
    #[error("requirement must be baselined before it can be delivered")]
    NotBaselined,
    #[error("review comment is required when rejecting a requirement")]
    EmptyReviewComment,
    #[error("requirement is not pending review")]
    NotUnderReview,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceCriterion {
    pub text: String,
}

impl AcceptanceCriterion {
    pub fn parse(text: &str) -> Result<Self, RequirementError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(RequirementError::EmptyCriterion);
        }
        Ok(Self { text: text.to_string() })
    }
}

pub fn parse_criteria(raw: &[String]) -> Result<Vec<AcceptanceCriterion>, RequirementError> {
    raw.iter().map(|c| AcceptanceCriterion::parse(c)).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRequirement {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    pub created_by: String,
}

impl NewRequirement {
    pub fn new(
        project_id: &str,
        title: &str,
        description: &str,
        criteria: &[String],
    ) -> Result<Self, RequirementError> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(RequirementError::EmptyProject);
        }
        let title = title.trim();
        if title.is_empty() {
            return Err(RequirementError::EmptyTitle);
        }
        // 按字符数计长度,避免中文标题被误判超长。
        if title.chars().count() > MAX_TITLE_LEN {
            return Err(RequirementError::TitleTooLong);
        }
        Ok(Self {
            project_id: project_id.to_string(),
            title: title.to_string(),
            description: description.trim().to_string(),
            acceptance_criteria: parse_criteria(criteria)?,
            created_by: String::new(),
        })
    }

    pub fn with_created_by(mut self, user_id: &str) -> Self {
        self.created_by = user_id.trim().to_string();
        self
    }
}

/// 按状态聚合的需求计数(项目仪表盘用)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusCounts {
    pub draft: u64,
    pub baselined: u64,
    pub delivered: u64,
    pub archived: u64,
}

impl StatusCounts {
    pub fn total(&self) -> u64 {
        self.draft + self.baselined + self.delivered + self.archived
    }

    /// 把一个状态计入对应桶。
    pub fn add(&mut self, status: RequirementStatus) {
        match status {
            RequirementStatus::Draft => self.draft += 1,
            RequirementStatus::Baselined => self.baselined += 1,
            RequirementStatus::Delivered => self.delivered += 1,
            RequirementStatus::Archived => self.archived += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequirementStatus {
    Draft,
    Baselined,
    Delivered,
    Archived,
}

impl RequirementStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Baselined => "BASELINED",
            Self::Delivered => "DELIVERED",
            Self::Archived => "ARCHIVED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "DRAFT" => Some(Self::Draft),
            "BASELINED" => Some(Self::Baselined),
            "DELIVERED" => Some(Self::Delivered),
            "ARCHIVED" => Some(Self::Archived),
            _ => None,
        }
    }
}

/// 不可变快照:一旦创建,内容永不改写;修订只追加新版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementVersion {
    pub version: u32,
    pub description: String,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub status: RequirementStatus,
    pub baseline_version: u32,
    pub versions: Vec<RequirementVersion>,
    pub deleted: bool,
    pub review_comment: Option<String>,
}

impl Requirement {
    pub fn create(id: &str, new: &NewRequirement) -> Self {
        Self {
            id: id.to_string(),
            project_id: new.project_id.clone(),
            title: new.title.clone(),
            status: RequirementStatus::Draft,
            baseline_version: 1,
            versions: vec![RequirementVersion {
                version: 1,
                description: new.description.clone(),
                acceptance_criteria: new.acceptance_criteria.clone(),
            }],
            deleted: false,
            review_comment: None,
        }
    }

    pub fn latest_version(&self) -> u32 {
        self.versions.last().map(|v| v.version).unwrap_or(0)
    }

    pub fn version(&self, n: u32) -> Option<&RequirementVersion> {
        self.versions.iter().find(|v| v.version == n)
    }

    pub fn baseline(&self) -> &RequirementVersion {
        self.version(self.baseline_version).expect("baseline always points to an existing version")
    }

    pub fn latest(&self) -> &RequirementVersion {
        self.versions.last().expect("a requirement always has at least version 1")
    }

    /// 追加新版本但不移动 baseline;归档后拒绝修订。
    pub fn revise(
        &mut self,
        description: &str,
        criteria: Vec<AcceptanceCriterion>,
    ) -> Result<u32, RequirementError> {
        if self.status == RequirementStatus::Archived {
            return Err(RequirementError::Archived);
        }
        let next = self.latest_version() + 1;
        self.versions.push(RequirementVersion {
            version: next,
            description: description.trim().to_string(),
            acceptance_criteria: criteria,
        });
        self.review_comment = None;
        Ok(next)
    }

    /// 评审不通过:记录原因,需求留在 DRAFT 待重评;仅对待评审草稿适用。
    pub fn reject_review(&mut self, reason: &str) -> Result<(), RequirementError> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(RequirementError::EmptyReviewComment);
        }
        match self.status {
            RequirementStatus::Draft => {
                self.review_comment = Some(reason.to_string());
                Ok(())
            }
            RequirementStatus::Archived => Err(RequirementError::Archived),
            RequirementStatus::Baselined | RequirementStatus::Delivered => {
                Err(RequirementError::NotUnderReview)
            }
        }
    }

    /// 首次定基把 Draft → Baselined。
    pub fn set_baseline(&mut self, version: u32) -> Result<(), RequirementError> {
        if self.version(version).is_none() {
            return Err(RequirementError::NoSuchVersion(version));
        }
        self.baseline_version = version;
        if self.status == RequirementStatus::Draft {
            self.status = RequirementStatus::Baselined;
        }
        self.review_comment = None;
        Ok(())
    }

    /// Baselined → Delivered,幂等;未定基线 → NotBaselined。
    pub fn deliver(&mut self) -> Result<(), RequirementError> {
        match self.status {
            RequirementStatus::Delivered => Ok(()),
            RequirementStatus::Baselined => {
                self.status = RequirementStatus::Delivered;
                Ok(())
            }
            RequirementStatus::Draft => Err(RequirementError::NotBaselined),
            RequirementStatus::Archived => Err(RequirementError::Archived),
        }
    }

    pub fn rename(&mut self, title: &str) -> Result<(), RequirementError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(RequirementError::EmptyTitle);
        }
        if title.chars().count() > MAX_TITLE_LEN {
            return Err(RequirementError::TitleTooLong);
        }
        self.title = title.to_string();
        Ok(())
    }

    pub fn archive(&mut self) {
        self.status = RequirementStatus::Archived;
    }

    /// 软删除后标题被释放(可重建同名)。
    pub fn soft_delete(&mut self) {
        self.deleted = true;
    }

    /// 仅未删除的需求参与标题唯一性判定。
    pub fn occupies_title(&self) -> bool {
        !self.deleted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crit(items: &[&str]) -> Vec<AcceptanceCriterion> {
        items.iter().map(|s| AcceptanceCriterion::parse(s).expect("valid")).collect()
    }

    fn new_req() -> NewRequirement {
        NewRequirement::new(
            "proj1",
            "  登录功能  ",
            "  用户可用邮箱登录  ",
            &["可用正确邮箱+密码登录".to_string(), "错误密码拒绝".to_string()],
        )
        .expect("valid")
    }

    #[test]
    fn new_requirement_trims_and_parses_criteria() {
        let n = new_req();
        assert_eq!(n.title, "登录功能");
        assert_eq!(n.description, "用户可用邮箱登录");
        assert_eq!(n.acceptance_criteria.len(), 2);
    }

    #[test]
    fn rejects_blank_title_project_and_empty_criterion() {
        assert_eq!(NewRequirement::new("p", "  ", "d", &[]), Err(RequirementError::EmptyTitle));
        assert_eq!(NewRequirement::new(" ", "t", "d", &[]), Err(RequirementError::EmptyProject));
        assert_eq!(
            NewRequirement::new("p", "t", "d", &["ok".to_string(), "  ".to_string()]),
            Err(RequirementError::EmptyCriterion)
        );
    }

    #[test]
    fn create_starts_at_v1_baseline_1_draft() {
        let r = Requirement::create("req-1", &new_req());
        assert_eq!(r.latest_version(), 1);
        assert_eq!(r.baseline_version, 1);
        assert_eq!(r.status, RequirementStatus::Draft);
        assert_eq!(r.baseline().version, 1);
        assert!(r.occupies_title());
    }

    #[test]
    fn revise_appends_monotonic_immutable_version() {
        let mut r = Requirement::create("req-1", &new_req());
        let v1_criteria = r.version(1).expect("v1").acceptance_criteria.clone();

        let n = r.revise("v2 描述", crit(&["新标准A"])).expect("revise");
        assert_eq!(n, 2);
        assert_eq!(r.latest_version(), 2);
        assert_eq!(r.version(1).expect("v1").acceptance_criteria, v1_criteria);
        assert_eq!(r.version(2).expect("v2").description, "v2 描述");

        let n3 = r.revise("v3", crit(&["X"])).expect("revise");
        assert_eq!(n3, 3);
    }

    #[test]
    fn revise_does_not_move_baseline() {
        let mut r = Requirement::create("req-1", &new_req());
        r.revise("v2", crit(&["X"])).expect("revise");
        assert_eq!(r.baseline_version, 1);
        assert_eq!(r.baseline().version, 1);
    }

    #[test]
    fn set_baseline_moves_pointer_and_promotes_status() {
        let mut r = Requirement::create("req-1", &new_req());
        r.revise("v2", crit(&["X"])).expect("revise");
        r.set_baseline(2).expect("set baseline");
        assert_eq!(r.baseline_version, 2);
        assert_eq!(r.status, RequirementStatus::Baselined);
    }

    #[test]
    fn set_baseline_to_unknown_version_errors() {
        let mut r = Requirement::create("req-1", &new_req());
        assert_eq!(r.set_baseline(9), Err(RequirementError::NoSuchVersion(9)));
        assert_eq!(r.baseline_version, 1);
    }

    #[test]
    fn archived_requirement_rejects_revise() {
        let mut r = Requirement::create("req-1", &new_req());
        r.archive();
        assert_eq!(r.status, RequirementStatus::Archived);
        assert_eq!(r.revise("v2", crit(&["X"])), Err(RequirementError::Archived));
        assert_eq!(r.latest_version(), 1);
    }

    #[test]
    fn rename_validates() {
        let mut r = Requirement::create("req-1", &new_req());
        assert!(r.rename("  新标题 ").is_ok());
        assert_eq!(r.title, "新标题");
        assert_eq!(r.rename("   "), Err(RequirementError::EmptyTitle));
    }

    #[test]
    fn soft_delete_frees_title() {
        let mut r = Requirement::create("req-1", &new_req());
        r.soft_delete();
        assert!(r.deleted);
        assert!(!r.occupies_title());
    }

    #[test]
    fn reject_review_records_reason_and_keeps_draft() {
        let mut r = Requirement::create("req-1", &new_req());
        r.reject_review("  验收标准不完整  ").expect("reject");
        assert_eq!(r.status, RequirementStatus::Draft);
        assert_eq!(r.review_comment.as_deref(), Some("验收标准不完整"));
    }

    #[test]
    fn reject_review_requires_reason() {
        let mut r = Requirement::create("req-1", &new_req());
        assert_eq!(r.reject_review("   "), Err(RequirementError::EmptyReviewComment));
        assert!(r.review_comment.is_none());
    }

    #[test]
    fn pass_then_baseline_clears_prior_rejection() {
        let mut r = Requirement::create("req-1", &new_req());
        r.reject_review("缺少边界场景").expect("reject");
        r.set_baseline(1).expect("baseline");
        assert_eq!(r.status, RequirementStatus::Baselined);
        assert!(r.review_comment.is_none());
    }

    #[test]
    fn revise_clears_prior_rejection() {
        let mut r = Requirement::create("req-1", &new_req());
        r.reject_review("缺少边界场景").expect("reject");
        r.revise("v2", crit(&["补充边界"])).expect("revise");
        assert!(r.review_comment.is_none());
    }

    #[test]
    fn cannot_reject_baselined_or_archived() {
        let mut r = Requirement::create("req-1", &new_req());
        r.set_baseline(1).expect("baseline");
        assert_eq!(r.reject_review("x"), Err(RequirementError::NotUnderReview));
        let mut a = Requirement::create("req-2", &new_req());
        a.archive();
        assert_eq!(a.reject_review("x"), Err(RequirementError::Archived));
    }

    #[test]
    fn status_str_roundtrip() {
        for s in [RequirementStatus::Draft, RequirementStatus::Baselined, RequirementStatus::Archived] {
            assert_eq!(RequirementStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(RequirementStatus::parse("WAT"), None);
    }
}
