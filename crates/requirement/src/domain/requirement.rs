use thiserror::Error;

/// 与 DB 列一致。
pub const MAX_TITLE_LEN: usize = 255;

/// 单条需求最多挂的标签数。
pub const MAX_TAGS: usize = 10;
/// 单个标签最大长度(按字符数,避免中文被误判)。
pub const MAX_TAG_LEN: usize = 32;

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
    #[error("invalid requirement priority: {0}")]
    InvalidPriority(String),
    #[error("invalid requirement type: {0}")]
    InvalidReqType(String),
    #[error("too many tags (max {MAX_TAGS})")]
    TooManyTags,
    #[error("tag too long: {0}")]
    TagTooLong(String),
    #[error("invalid due date (expect YYYY-MM-DD): {0}")]
    InvalidDueDate(String),
    #[error("invalid work status: {0}")]
    InvalidWorkStatus(String),
    #[error("invalid stage: {0}")]
    InvalidStage(String),
    #[error("invalid stage status: {0}")]
    InvalidStageStatus(String),
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
    pub priority: RequirementPriority,
    pub req_type: RequirementType,
    /// 已清洗的标签(见 `normalize_tags`)。
    pub tags: Vec<String>,
    /// 已校验的截止日期 `YYYY-MM-DD`。
    pub due_date: Option<String>,
    /// 父需求 id;存在性/同项目校验在应用层做(需要仓储)。
    pub parent_id: Option<String>,
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
            priority: RequirementPriority::default(),
            req_type: RequirementType::default(),
            tags: Vec::new(),
            due_date: None,
            parent_id: None,
        })
    }

    pub fn with_created_by(mut self, user_id: &str) -> Self {
        self.created_by = user_id.trim().to_string();
        self
    }

    pub fn with_priority(mut self, priority: RequirementPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_req_type(mut self, req_type: RequirementType) -> Self {
        self.req_type = req_type;
        self
    }

    /// 携带已清洗的标签(调用方先过 `normalize_tags`)。
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// 携带已校验的截止日期(调用方先过 `parse_due_date`)。
    pub fn with_due_date(mut self, due_date: Option<String>) -> Self {
        self.due_date = due_date;
        self
    }

    /// 携带父需求 id(应用层已校验存在性/同项目)。
    pub fn with_parent(mut self, parent_id: Option<String>) -> Self {
        self.parent_id = parent_id;
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

/// 需求优先级,P0 最高;缺省 P2。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RequirementPriority {
    P0,
    P1,
    #[default]
    P2,
    P3,
}

impl RequirementPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::P0 => "P0",
            Self::P1 => "P1",
            Self::P2 => "P2",
            Self::P3 => "P3",
        }
    }

    /// 归一化(trim + 大写)后匹配;非法值 → None。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "P0" => Some(Self::P0),
            "P1" => Some(Self::P1),
            "P2" => Some(Self::P2),
            "P3" => Some(Self::P3),
            _ => None,
        }
    }
}

/// 解析优先级;非法值报校验错(HTTP 层映射为 400)。
pub fn parse_priority(s: &str) -> Result<RequirementPriority, RequirementError> {
    RequirementPriority::parse(s)
        .ok_or_else(|| RequirementError::InvalidPriority(s.trim().to_string()))
}

/// 需求类型;缺省 FEATURE。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RequirementType {
    #[default]
    Feature,
    Enhancement,
    TechDebt,
    Bugfix,
}

impl RequirementType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Feature => "FEATURE",
            Self::Enhancement => "ENHANCEMENT",
            Self::TechDebt => "TECH_DEBT",
            Self::Bugfix => "BUGFIX",
        }
    }

    /// 归一化(trim + 大写)后匹配;非法值 → None。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "FEATURE" => Some(Self::Feature),
            "ENHANCEMENT" => Some(Self::Enhancement),
            "TECH_DEBT" => Some(Self::TechDebt),
            "BUGFIX" => Some(Self::Bugfix),
            _ => None,
        }
    }
}

/// 解析需求类型;非法值报校验错(HTTP 层映射为 400)。
pub fn parse_req_type(s: &str) -> Result<RequirementType, RequirementError> {
    RequirementType::parse(s).ok_or_else(|| RequirementError::InvalidReqType(s.trim().to_string()))
}

/// 工作推进状态(开发/测试共用);缺省未开始。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WorkStatus {
    #[default]
    NotStarted,
    InProgress,
    Done,
}

impl WorkStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotStarted => "NOT_STARTED",
            Self::InProgress => "IN_PROGRESS",
            Self::Done => "DONE",
        }
    }

    /// 归一化(trim + 大写)后匹配;非法值 → None。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "NOT_STARTED" => Some(Self::NotStarted),
            "IN_PROGRESS" => Some(Self::InProgress),
            "DONE" => Some(Self::Done),
            _ => None,
        }
    }
}

/// 解析工作状态;非法值报校验错(HTTP 层映射为 400)。
pub fn parse_work_status(s: &str) -> Result<WorkStatus, RequirementError> {
    WorkStatus::parse(s).ok_or_else(|| RequirementError::InvalidWorkStatus(s.trim().to_string()))
}

/// 需求流水线阶段,顺序固定:创建 → 审核 → 评审 → 开发 → 测试 → 验收 → 交付。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Created,
    Audit,
    Review,
    Dev,
    Test,
    Acceptance,
    Delivery,
}

impl Stage {
    /// 全部阶段,按流水线顺序。
    pub const ALL: [Stage; 7] = [
        Self::Created,
        Self::Audit,
        Self::Review,
        Self::Dev,
        Self::Test,
        Self::Acceptance,
        Self::Delivery,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "CREATED",
            Self::Audit => "AUDIT",
            Self::Review => "REVIEW",
            Self::Dev => "DEV",
            Self::Test => "TEST",
            Self::Acceptance => "ACCEPTANCE",
            Self::Delivery => "DELIVERY",
        }
    }

    /// 归一化(trim + 大写)后匹配;非法值 → None。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "CREATED" => Some(Self::Created),
            "AUDIT" => Some(Self::Audit),
            "REVIEW" => Some(Self::Review),
            "DEV" => Some(Self::Dev),
            "TEST" => Some(Self::Test),
            "ACCEPTANCE" => Some(Self::Acceptance),
            "DELIVERY" => Some(Self::Delivery),
            _ => None,
        }
    }
}

/// 解析阶段;非法值报校验错(HTTP 层映射为 400)。
pub fn parse_stage(s: &str) -> Result<Stage, RequirementError> {
    Stage::parse(s).ok_or_else(|| RequirementError::InvalidStage(s.trim().to_string()))
}

/// 阶段推进状态;缺省待处理。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StageStatus {
    #[default]
    Pending,
    InProgress,
    Done,
    Skipped,
}

impl StageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::InProgress => "IN_PROGRESS",
            Self::Done => "DONE",
            Self::Skipped => "SKIPPED",
        }
    }

    /// 归一化(trim + 大写)后匹配;非法值 → None。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "PENDING" => Some(Self::Pending),
            "IN_PROGRESS" => Some(Self::InProgress),
            "DONE" => Some(Self::Done),
            "SKIPPED" => Some(Self::Skipped),
            _ => None,
        }
    }
}

/// 解析阶段状态;非法值报校验错(HTTP 层映射为 400)。
pub fn parse_stage_status(s: &str) -> Result<StageStatus, RequirementError> {
    StageStatus::parse(s).ok_or_else(|| RequirementError::InvalidStageStatus(s.trim().to_string()))
}

/// 一条阶段行:状态 + 计划起止日期(`YYYY-MM-DD`)+ 实际起止时间(epoch 毫秒)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageRow {
    pub stage: Stage,
    pub status: StageStatus,
    pub planned_start: Option<String>,
    pub planned_end: Option<String>,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
}

impl StageRow {
    /// 该阶段的默认行:PENDING、无计划、无时间戳。
    pub fn pending(stage: Stage) -> Self {
        Self {
            stage,
            status: StageStatus::Pending,
            planned_start: None,
            planned_end: None,
            started_at_ms: None,
            finished_at_ms: None,
        }
    }

    /// 推进状态并盖章:进 IN_PROGRESS 记开始时间;进 DONE 补齐开始/完成时间。
    /// 时间戳先写先得,重复推进不覆盖;回 PENDING 不清空;SKIPPED 任意状态可进,不盖章。
    pub fn set_status(&mut self, status: StageStatus, now_ms: i64) {
        self.status = status;
        match status {
            StageStatus::Pending | StageStatus::Skipped => {}
            StageStatus::InProgress => {
                self.started_at_ms.get_or_insert(now_ms);
            }
            StageStatus::Done => {
                self.started_at_ms.get_or_insert(now_ms);
                self.finished_at_ms.get_or_insert(now_ms);
            }
        }
    }
}

/// 把稀疏的阶段行归一成固定 7 行(按流水线顺序,缺失的补 PENDING 默认行);
/// 旧数据没回填也能得到完整流水线。
pub fn fill_stages(rows: &[StageRow]) -> Vec<StageRow> {
    Stage::ALL
        .iter()
        .map(|s| {
            rows.iter().find(|r| r.stage == *s).cloned().unwrap_or_else(|| StageRow::pending(*s))
        })
        .collect()
}

/// 阶段逾期判定(实时计算,不落库):设了计划结束日且未跳过——
/// 已完成的看完成日期是否晚于计划;未完成的看今天是否已过计划(当天不算)。
pub fn stage_overdue(row: &StageRow, today: &str) -> bool {
    let Some(end) = row.planned_end.as_deref() else { return false };
    if row.status == StageStatus::Skipped {
        return false;
    }
    match row.finished_at_ms {
        Some(ms) => date_from_epoch_ms(ms).as_str() > end,
        None => row.status != StageStatus::Done && today > end,
    }
}

/// 标签清洗:trim、去空、按首现顺序去重;超过 `MAX_TAGS` 个或单个超 `MAX_TAG_LEN` 报校验错。
pub fn normalize_tags(raw: &[String]) -> Result<Vec<String>, RequirementError> {
    let mut out: Vec<String> = Vec::new();
    for t in raw {
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        if t.chars().count() > MAX_TAG_LEN {
            return Err(RequirementError::TagTooLong(t.to_string()));
        }
        if out.iter().any(|e| e == t) {
            continue;
        }
        out.push(t.to_string());
    }
    if out.len() > MAX_TAGS {
        return Err(RequirementError::TooManyTags);
    }
    Ok(out)
}

/// 校验截止日期串:严格 `YYYY-MM-DD`,月日范围合法(含闰年判定)。
pub fn parse_due_date(s: &str) -> Result<String, RequirementError> {
    let s = s.trim();
    let bad = || RequirementError::InvalidDueDate(s.to_string());
    let bytes = s.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(bad());
    }
    let digits = |r: std::ops::Range<usize>| -> Result<u32, RequirementError> {
        let part = &s[r];
        if !part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        part.parse().map_err(|_| bad())
    };
    let year = digits(0..4)?;
    let month = digits(5..7)?;
    let day = digits(8..10)?;
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return Err(bad());
    }
    Ok(s.to_string())
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            if leap {
                29
            } else {
                28
            }
        }
    }
}

/// epoch 毫秒 → UTC 日期串 `YYYY-MM-DD`(civil-from-days 算法,纯函数便于测试)。
pub fn date_from_epoch_ms(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// 变更日志:待追加的一条字段级变更(时间由存储层盖章)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChange {
    pub changed_by: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

/// 变更日志:已落库的一条记录(读侧)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeEntry {
    pub changed_at_ms: i64,
    pub changed_by: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
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
    pub priority: RequirementPriority,
    pub req_type: RequirementType,
    pub tags: Vec<String>,
    pub parent_id: Option<String>,
    /// 截止日期 `YYYY-MM-DD`;是否逾期由 `is_overdue` 实时计算,不落库。
    pub due_date: Option<String>,
    /// 创建/更新时间(epoch 毫秒,全库口径);由存储层盖章,内存值 0 表示尚未落库。
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub dev_status: WorkStatus,
    pub test_status: WorkStatus,
    pub dev_started_at_ms: Option<i64>,
    pub dev_finished_at_ms: Option<i64>,
    pub test_started_at_ms: Option<i64>,
    pub test_finished_at_ms: Option<i64>,
    /// 7 阶段流水线,恒为全部阶段、按顺序(仓储层用 `fill_stages` 补齐缺行)。
    pub stages: Vec<StageRow>,
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
            priority: new.priority,
            req_type: new.req_type,
            tags: new.tags.clone(),
            parent_id: new.parent_id.clone(),
            due_date: new.due_date.clone(),
            created_at_ms: 0,
            updated_at_ms: 0,
            dev_status: WorkStatus::NotStarted,
            test_status: WorkStatus::NotStarted,
            dev_started_at_ms: None,
            dev_finished_at_ms: None,
            test_started_at_ms: None,
            test_finished_at_ms: None,
            stages: fill_stages(&[]),
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

    /// 逾期判定(实时计算,不落库):截止日期规则(设了截止日期、已过 `today`
    /// 且尚未交付/归档)或任一阶段逾期(见 `stage_overdue`)。
    pub fn is_overdue(&self, today: &str) -> bool {
        let due_overdue = match (&self.due_date, self.status) {
            (_, RequirementStatus::Delivered | RequirementStatus::Archived) => false,
            (Some(due), _) => due.as_str() < today,
            (None, _) => false,
        };
        due_overdue || self.stages.iter().any(|row| stage_overdue(row, today))
    }

    /// 当前所处阶段:按顺序第一个既未完成也未跳过的;全部完成 → 交付。
    pub fn current_stage(&self) -> Stage {
        self.stages
            .iter()
            .find(|r| !matches!(r.status, StageStatus::Done | StageStatus::Skipped))
            .map(|r| r.stage)
            .unwrap_or(Stage::Delivery)
    }

    pub fn stage_row(&self, stage: Stage) -> Option<&StageRow> {
        self.stages.iter().find(|r| r.stage == stage)
    }

    pub fn stage_row_mut(&mut self, stage: Stage) -> Option<&mut StageRow> {
        self.stages.iter_mut().find(|r| r.stage == stage)
    }

    /// 推进某阶段状态并盖章;规则见 `StageRow::set_status`。
    pub fn set_stage(&mut self, stage: Stage, status: StageStatus, now_ms: i64) {
        if let Some(row) = self.stage_row_mut(stage) {
            row.set_status(status, now_ms);
        }
    }

    /// 推进开发状态并盖章:首次进 IN_PROGRESS 记开始时间;进 DONE 记完成时间
    /// (若开始时间尚缺一并补上);重复推进不覆盖已有时间戳。
    pub fn set_dev_status(&mut self, status: WorkStatus, now_ms: i64) {
        self.dev_status = status;
        stamp(status, now_ms, &mut self.dev_started_at_ms, &mut self.dev_finished_at_ms);
    }

    /// 推进测试状态并盖章;规则同 `set_dev_status`。
    pub fn set_test_status(&mut self, status: WorkStatus, now_ms: i64) {
        self.test_status = status;
        stamp(status, now_ms, &mut self.test_started_at_ms, &mut self.test_finished_at_ms);
    }
}

fn stamp(status: WorkStatus, now_ms: i64, started: &mut Option<i64>, finished: &mut Option<i64>) {
    match status {
        WorkStatus::NotStarted => {}
        WorkStatus::InProgress => {
            started.get_or_insert(now_ms);
        }
        WorkStatus::Done => {
            started.get_or_insert(now_ms);
            finished.get_or_insert(now_ms);
        }
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
    fn create_defaults_priority_p2_and_type_feature() {
        let r = Requirement::create("req-1", &new_req());
        assert_eq!(r.priority, RequirementPriority::P2);
        assert_eq!(r.req_type, RequirementType::Feature);
    }

    #[test]
    fn create_carries_explicit_priority_and_type() {
        let n =
            new_req().with_priority(RequirementPriority::P0).with_req_type(RequirementType::Bugfix);
        let r = Requirement::create("req-1", &n);
        assert_eq!(r.priority, RequirementPriority::P0);
        assert_eq!(r.req_type, RequirementType::Bugfix);
    }

    #[test]
    fn priority_str_roundtrip_and_normalization() {
        for p in [
            RequirementPriority::P0,
            RequirementPriority::P1,
            RequirementPriority::P2,
            RequirementPriority::P3,
        ] {
            assert_eq!(RequirementPriority::parse(p.as_str()), Some(p));
        }
        // trim + 大写归一。
        assert_eq!(RequirementPriority::parse("  p1 "), Some(RequirementPriority::P1));
        assert_eq!(RequirementPriority::parse("P9"), None);
        assert_eq!(parse_priority("p3"), Ok(RequirementPriority::P3));
        assert_eq!(parse_priority(" P9 "), Err(RequirementError::InvalidPriority("P9".into())));
    }

    #[test]
    fn req_type_str_roundtrip_and_normalization() {
        for t in [
            RequirementType::Feature,
            RequirementType::Enhancement,
            RequirementType::TechDebt,
            RequirementType::Bugfix,
        ] {
            assert_eq!(RequirementType::parse(t.as_str()), Some(t));
        }
        assert_eq!(RequirementType::parse(" tech_debt "), Some(RequirementType::TechDebt));
        assert_eq!(RequirementType::parse("EPIC"), None);
        assert_eq!(parse_req_type("bugfix"), Ok(RequirementType::Bugfix));
        assert_eq!(parse_req_type(" EPIC "), Err(RequirementError::InvalidReqType("EPIC".into())));
    }

    #[test]
    fn work_status_str_roundtrip_and_normalization() {
        for s in [WorkStatus::NotStarted, WorkStatus::InProgress, WorkStatus::Done] {
            assert_eq!(WorkStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(WorkStatus::parse(" in_progress "), Some(WorkStatus::InProgress));
        assert_eq!(WorkStatus::parse("PAUSED"), None);
        assert_eq!(parse_work_status("done"), Ok(WorkStatus::Done));
        assert_eq!(
            parse_work_status(" WAT "),
            Err(RequirementError::InvalidWorkStatus("WAT".into()))
        );
    }

    #[test]
    fn normalize_tags_trims_dedupes_preserving_order() {
        let raw = vec!["  api ".to_string(), "".to_string(), "web".to_string(), "api".to_string()];
        assert_eq!(normalize_tags(&raw), Ok(vec!["api".to_string(), "web".to_string()]));
        assert_eq!(normalize_tags(&[]), Ok(vec![]));
    }

    #[test]
    fn normalize_tags_rejects_too_many_or_too_long() {
        let many: Vec<String> = (0..11).map(|i| format!("t{i}")).collect();
        assert_eq!(normalize_tags(&many), Err(RequirementError::TooManyTags));
        // 去重后 10 个恰好合法。
        let ten: Vec<String> = (0..10).map(|i| format!("t{i}")).collect();
        assert_eq!(normalize_tags(&ten).expect("ok").len(), 10);
        let long = "标".repeat(33);
        assert_eq!(normalize_tags(&[long.clone()]), Err(RequirementError::TagTooLong(long)));
        // 32 个字符(含中文)合法。
        assert!(normalize_tags(&["标".repeat(32)]).is_ok());
    }

    #[test]
    fn parse_due_date_validates_format_and_ranges() {
        assert_eq!(parse_due_date(" 2026-07-11 "), Ok("2026-07-11".to_string()));
        assert_eq!(parse_due_date("2024-02-29"), Ok("2024-02-29".to_string())); // 闰年
        for bad in [
            "2026/07/11",
            "2026-13-01",
            "2026-00-10",
            "2026-02-30",
            "26-07-11",
            "abcd-ef-gh",
            "2023-02-29",
        ] {
            assert_eq!(parse_due_date(bad), Err(RequirementError::InvalidDueDate(bad.into())));
        }
    }

    #[test]
    fn date_from_epoch_ms_converts_utc() {
        assert_eq!(date_from_epoch_ms(0), "1970-01-01");
        // 2026-07-11 00:00:00 UTC = 1783728000s。
        assert_eq!(date_from_epoch_ms(1_783_728_000_000), "2026-07-11");
        // 前一毫秒仍是 7 月 10 日。
        assert_eq!(date_from_epoch_ms(1_783_727_999_999), "2026-07-10");
        // 闰日:2024-02-29 12:00 UTC。
        assert_eq!(date_from_epoch_ms(1_709_208_000_000), "2024-02-29");
    }

    #[test]
    fn overdue_requires_past_due_date_and_open_status() {
        let mut r = Requirement::create("req-1", &new_req());
        assert!(!r.is_overdue("2026-07-11")); // 无截止日期
        r.due_date = Some("2026-07-10".to_string());
        assert!(r.is_overdue("2026-07-11")); // 已过期
        assert!(!r.is_overdue("2026-07-10")); // 当天不算逾期
        assert!(!r.is_overdue("2026-07-09")); // 未到期
        r.status = RequirementStatus::Delivered;
        assert!(!r.is_overdue("2026-07-11")); // 已交付不再逾期
        r.status = RequirementStatus::Archived;
        assert!(!r.is_overdue("2026-07-11")); // 已归档不再逾期
    }

    #[test]
    fn dev_status_stamps_started_and_finished_once() {
        let mut r = Requirement::create("req-1", &new_req());
        assert_eq!(r.dev_status, WorkStatus::NotStarted);
        r.set_dev_status(WorkStatus::InProgress, 100);
        assert_eq!(r.dev_started_at_ms, Some(100));
        assert_eq!(r.dev_finished_at_ms, None);
        // 重复推进不覆盖。
        r.set_dev_status(WorkStatus::InProgress, 200);
        assert_eq!(r.dev_started_at_ms, Some(100));
        r.set_dev_status(WorkStatus::Done, 300);
        assert_eq!(r.dev_started_at_ms, Some(100));
        assert_eq!(r.dev_finished_at_ms, Some(300));
        r.set_dev_status(WorkStatus::Done, 400);
        assert_eq!(r.dev_finished_at_ms, Some(300));
    }

    #[test]
    fn test_status_done_backfills_started_when_missing() {
        let mut r = Requirement::create("req-1", &new_req());
        r.set_test_status(WorkStatus::Done, 500);
        assert_eq!(r.test_status, WorkStatus::Done);
        assert_eq!(r.test_started_at_ms, Some(500));
        assert_eq!(r.test_finished_at_ms, Some(500));
    }

    #[test]
    fn create_carries_tags_due_date_and_parent() {
        let n = new_req()
            .with_tags(vec!["api".to_string()])
            .with_due_date(Some("2026-08-01".to_string()))
            .with_parent(Some("req-0".to_string()));
        let r = Requirement::create("req-1", &n);
        assert_eq!(r.tags, vec!["api".to_string()]);
        assert_eq!(r.due_date.as_deref(), Some("2026-08-01"));
        assert_eq!(r.parent_id.as_deref(), Some("req-0"));
    }

    #[test]
    fn status_str_roundtrip() {
        for s in
            [RequirementStatus::Draft, RequirementStatus::Baselined, RequirementStatus::Archived]
        {
            assert_eq!(RequirementStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(RequirementStatus::parse("WAT"), None);
    }

    #[test]
    fn stage_and_stage_status_roundtrip_and_normalization() {
        for s in Stage::ALL {
            assert_eq!(Stage::parse(s.as_str()), Some(s));
        }
        assert_eq!(Stage::parse(" dev "), Some(Stage::Dev));
        assert_eq!(Stage::parse("DESIGN"), None);
        assert_eq!(parse_stage("review"), Ok(Stage::Review));
        assert_eq!(parse_stage(" 上线 "), Err(RequirementError::InvalidStage("上线".into())));

        for s in
            [StageStatus::Pending, StageStatus::InProgress, StageStatus::Done, StageStatus::Skipped]
        {
            assert_eq!(StageStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(StageStatus::parse(" skipped "), Some(StageStatus::Skipped));
        assert_eq!(StageStatus::parse("PAUSED"), None);
        assert_eq!(parse_stage_status("done"), Ok(StageStatus::Done));
        assert_eq!(
            parse_stage_status(" WAT "),
            Err(RequirementError::InvalidStageStatus("WAT".into()))
        );
    }

    #[test]
    fn fill_stages_completes_and_orders_all_seven() {
        // 空输入 → 7 行 PENDING,按流水线顺序。
        let filled = fill_stages(&[]);
        assert_eq!(filled.len(), 7);
        assert_eq!(filled.iter().map(|r| r.stage).collect::<Vec<_>>(), Stage::ALL);
        assert!(filled.iter().all(|r| r.status == StageStatus::Pending));

        // 乱序稀疏输入:已有行保留,缺行补默认。
        let mut dev = StageRow::pending(Stage::Dev);
        dev.set_status(StageStatus::InProgress, 100);
        let mut created = StageRow::pending(Stage::Created);
        created.set_status(StageStatus::Done, 50);
        let filled = fill_stages(&[dev.clone(), created.clone()]);
        assert_eq!(filled.iter().map(|r| r.stage).collect::<Vec<_>>(), Stage::ALL);
        assert_eq!(filled[0], created);
        assert_eq!(filled[3], dev);
        assert_eq!(filled[1], StageRow::pending(Stage::Audit));
    }

    // 2026-07-11 00:00:00 UTC = 1_783_728_000_000 毫秒(见 date_from_epoch_ms 测试)。
    const JUL_11_MS: i64 = 1_783_728_000_000;

    #[test]
    fn stage_overdue_matrix() {
        let today = "2026-07-11";
        let mut row = StageRow::pending(Stage::Dev);
        // 无计划结束日 → 不逾期。
        assert!(!stage_overdue(&row, today));

        // 未完成且今天已过计划(当天不算)。
        row.planned_end = Some("2026-07-10".to_string());
        assert!(stage_overdue(&row, today));
        assert!(!stage_overdue(&row, "2026-07-10"));
        assert!(!stage_overdue(&row, "2026-07-09"));

        // 跳过的阶段不算逾期。
        row.set_status(StageStatus::Skipped, JUL_11_MS);
        assert!(stage_overdue(&StageRow { status: StageStatus::InProgress, ..row.clone() }, today));
        assert!(!stage_overdue(&row, today));

        // 按时完成:完成日 ≤ 计划 → 不逾期,哪怕今天已过计划。
        let mut done = StageRow::pending(Stage::Test);
        done.planned_end = Some("2026-07-10".to_string());
        done.set_status(StageStatus::Done, JUL_11_MS - 86_400_000); // 完成于 07-10
        assert!(!stage_overdue(&done, "2026-08-01"));

        // 迟完成:完成日 > 计划 → 永久逾期(与 today 无关)。
        let mut late = StageRow::pending(Stage::Test);
        late.planned_end = Some("2026-07-10".to_string());
        late.set_status(StageStatus::Done, JUL_11_MS); // 完成于 07-11
        assert!(stage_overdue(&late, "2026-07-01"));

        // 状态 DONE 但完成时间缺失(理论不该出现)→ 不按「未完成」误判。
        let odd = StageRow {
            status: StageStatus::Done,
            finished_at_ms: None,
            ..StageRow::pending(Stage::Dev)
        };
        let odd = StageRow { planned_end: Some("2026-07-01".to_string()), ..odd };
        assert!(!stage_overdue(&odd, today));
    }

    #[test]
    fn requirement_overdue_combines_due_date_and_stage_rules() {
        let mut r = Requirement::create("req-1", &new_req());
        assert!(!r.is_overdue("2026-07-11"));
        // 仅阶段逾期(无截止日期)也算需求逾期。
        r.stage_row_mut(Stage::Dev).expect("dev row").planned_end = Some("2026-07-01".to_string());
        assert!(r.is_overdue("2026-07-11"));
        // 已交付豁免截止日期规则,但阶段迟完成仍算逾期。
        r.status = RequirementStatus::Delivered;
        assert!(r.is_overdue("2026-07-11"));
        // 阶段按时完成后不再逾期。
        r.set_stage(Stage::Dev, StageStatus::Done, 1_751_328_000_000); // 完成于 2025-07-01,早于计划
        assert!(!r.is_overdue("2026-07-11"));
    }

    #[test]
    fn current_stage_walks_pipeline_in_order() {
        let mut r = Requirement::create("req-1", &new_req());
        assert_eq!(r.current_stage(), Stage::Created);
        r.set_stage(Stage::Created, StageStatus::Done, 1);
        assert_eq!(r.current_stage(), Stage::Audit);
        // SKIPPED 视同越过。
        r.set_stage(Stage::Audit, StageStatus::Skipped, 2);
        assert_eq!(r.current_stage(), Stage::Review);
        // IN_PROGRESS 仍停在当前阶段。
        r.set_stage(Stage::Review, StageStatus::InProgress, 3);
        assert_eq!(r.current_stage(), Stage::Review);
        for s in Stage::ALL {
            r.set_stage(s, StageStatus::Done, 4);
        }
        assert_eq!(r.current_stage(), Stage::Delivery);
    }

    #[test]
    fn stage_stamps_are_first_write_wins() {
        let mut r = Requirement::create("req-1", &new_req());
        r.set_stage(Stage::Dev, StageStatus::InProgress, 100);
        let dev = |r: &Requirement| r.stage_row(Stage::Dev).expect("dev row").clone();
        assert_eq!(dev(&r).started_at_ms, Some(100));
        assert_eq!(dev(&r).finished_at_ms, None);
        // 重复推进不覆盖。
        r.set_stage(Stage::Dev, StageStatus::InProgress, 200);
        assert_eq!(dev(&r).started_at_ms, Some(100));
        // 回 PENDING 不清空时间戳。
        r.set_stage(Stage::Dev, StageStatus::Pending, 300);
        assert_eq!(dev(&r).status, StageStatus::Pending);
        assert_eq!(dev(&r).started_at_ms, Some(100));
        r.set_stage(Stage::Dev, StageStatus::Done, 400);
        assert_eq!(dev(&r).started_at_ms, Some(100));
        assert_eq!(dev(&r).finished_at_ms, Some(400));
        r.set_stage(Stage::Dev, StageStatus::Done, 500);
        assert_eq!(dev(&r).finished_at_ms, Some(400));
        // DONE 后仍可 SKIPPED(任意状态可进),时间戳保留。
        r.set_stage(Stage::Dev, StageStatus::Skipped, 600);
        assert_eq!(dev(&r).status, StageStatus::Skipped);
        assert_eq!(dev(&r).finished_at_ms, Some(400));
        // 直接 DONE 补齐开始时间。
        r.set_stage(Stage::Test, StageStatus::Done, 700);
        let test = r.stage_row(Stage::Test).expect("test row");
        assert_eq!(test.started_at_ms, Some(700));
        assert_eq!(test.finished_at_ms, Some(700));
    }
}
