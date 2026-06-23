//! 需求领域模型:聚合 + 不可变版本快照 + baseline 指针。
//!
//! 设计取舍(线性快照 + baseline):
//! - **标题(title)是稳定身份**:在项目内唯一(忽略软删除),可 `rename`;
//! - **版本快照承载演进内容**:`description` + `acceptance_criteria` 随每次 `revise` 冻结为
//!   一个新版本,历史版本不可改写(make illegal states unrepresentable);
//! - **baseline 是显式决策**:`revise` 只追加版本、**不动基线**;`set_baseline` 才移动指针。
//!   这正是"多版本"的价值——稳定基线对外可见,新版本可在后台并行起草。

use thiserror::Error;

/// 需求标题长度上限(与 DB 列一致)。
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
}

/// 一条验收标准(已校验:非空)。后续 task / verification 上下文据此拆分与验收。
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

/// 把一组原始字符串解析成已校验的验收标准(任一为空即整体失败)。
pub fn parse_criteria(raw: &[String]) -> Result<Vec<AcceptanceCriterion>, RequirementError> {
    raw.iter().map(|c| AcceptanceCriterion::parse(c)).collect()
}

/// 创建需求的入站请求(尚无 id)。构造即校验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRequirement {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
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
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequirementStatus {
    /// 起草中(尚未定基线)。
    Draft,
    /// 已定基线(baseline 指向某个版本)。
    Baselined,
    /// 已交付(基线后所有验收标准经验证完整性达成)。
    Delivered,
    /// 已归档(冻结,拒绝再修订)。
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

/// 不可变版本快照。一旦创建,内容永不改写;修订只追加新版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementVersion {
    pub version: u32,
    pub description: String,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
}

/// 需求聚合。`title` 是稳定身份;`versions` 是单调递增的不可变快照序列;
/// `baseline_version` 指向当前生效版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub status: RequirementStatus,
    pub baseline_version: u32,
    pub versions: Vec<RequirementVersion>,
    pub deleted: bool,
}

impl Requirement {
    /// 由 `NewRequirement` 建初版:version 1、baseline=1、Draft、未删除。
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
        }
    }

    pub fn latest_version(&self) -> u32 {
        self.versions.last().map(|v| v.version).unwrap_or(0)
    }

    pub fn version(&self, n: u32) -> Option<&RequirementVersion> {
        self.versions.iter().find(|v| v.version == n)
    }

    /// 当前基线版本(不变量:始终存在)。
    pub fn baseline(&self) -> &RequirementVersion {
        self.version(self.baseline_version).expect("baseline always points to an existing version")
    }

    /// 最新(版本号最大)版本。
    pub fn latest(&self) -> &RequirementVersion {
        self.versions.last().expect("a requirement always has at least version 1")
    }

    /// 修订:追加一个新的不可变版本(version = latest+1),返回新版本号。
    /// **不移动 baseline**;归档后拒绝修订。
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
        Ok(next)
    }

    /// 把基线指向一个**已存在**的版本;首次定基把 Draft → Baselined。
    pub fn set_baseline(&mut self, version: u32) -> Result<(), RequirementError> {
        if self.version(version).is_none() {
            return Err(RequirementError::NoSuchVersion(version));
        }
        self.baseline_version = version;
        if self.status == RequirementStatus::Draft {
            self.status = RequirementStatus::Baselined;
        }
        Ok(())
    }

    /// 标记交付:基线后所有验收标准经验证达成时调用,Baselined → Delivered。
    /// 幂等(已 Delivered 直接成功);未定基线 → NotBaselined;已归档拒绝。
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

    /// 软删除:置 deleted,不物理移除。删除后标题被释放(可重建同名)。
    pub fn soft_delete(&mut self) {
        self.deleted = true;
    }

    /// 是否参与"标题唯一性"判定:仅未删除的需求算数。
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
        // 历史版本未被改写
        assert_eq!(r.version(1).expect("v1").acceptance_criteria, v1_criteria);
        assert_eq!(r.version(2).expect("v2").description, "v2 描述");

        let n3 = r.revise("v3", crit(&["X"])).expect("revise");
        assert_eq!(n3, 3);
    }

    #[test]
    fn revise_does_not_move_baseline() {
        let mut r = Requirement::create("req-1", &new_req());
        r.revise("v2", crit(&["X"])).expect("revise");
        // 基线仍钉在 v1,直到显式移动
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
        assert_eq!(r.baseline_version, 1); // 不变
    }

    #[test]
    fn archived_requirement_rejects_revise() {
        let mut r = Requirement::create("req-1", &new_req());
        r.archive();
        assert_eq!(r.status, RequirementStatus::Archived);
        assert_eq!(r.revise("v2", crit(&["X"])), Err(RequirementError::Archived));
        assert_eq!(r.latest_version(), 1); // 未追加
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
    fn status_str_roundtrip() {
        for s in [RequirementStatus::Draft, RequirementStatus::Baselined, RequirementStatus::Archived] {
            assert_eq!(RequirementStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(RequirementStatus::parse("WAT"), None);
    }
}
