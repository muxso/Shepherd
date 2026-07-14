use std::collections::BTreeMap;

use thiserror::Error;

/// Matches the DB column width.
pub const MAX_TITLE_LEN: usize = 255;

/// Max tags per requirement.
pub const MAX_TAGS: usize = 10;
/// Max tag length in chars (not bytes, so CJK is not over-counted).
pub const MAX_TAG_LEN: usize = 32;

/// Custom fields: max key count / key length / value length (in chars).
pub const MAX_CUSTOM_FIELDS: usize = 32;
pub const MAX_CUSTOM_FIELD_KEY_LEN: usize = 64;
pub const MAX_CUSTOM_FIELD_VALUE_LEN: usize = 2000;

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
    #[error("too many custom fields (max {MAX_CUSTOM_FIELDS})")]
    TooManyCustomFields,
    #[error("custom field key must not be empty")]
    EmptyCustomFieldKey,
    #[error("custom field key too long: {0}")]
    CustomFieldKeyTooLong(String),
    #[error("custom field value too long for key: {0}")]
    CustomFieldValueTooLong(String),
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
    /// Normalized tags (see `normalize_tags`).
    pub tags: Vec<String>,
    /// Validated due date, `YYYY-MM-DD`.
    pub due_date: Option<String>,
    /// Parent requirement id; existence/same-project checks happen in the
    /// application layer (they need the repository).
    pub parent_id: Option<String>,
    /// Validated custom field values (see `normalize_custom_fields`); field
    /// definitions are managed by the project template.
    pub custom_fields: BTreeMap<String, String>,
    /// Module id (shared ms_module project-level module tree); empty = unfiled.
    pub module_id: String,
    /// Selected project skill ids (dispatched to the agent runtime as composed instructions).
    pub skill_ids: Vec<String>,
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
        // Count chars, not bytes, so CJK titles are not flagged as too long.
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
            custom_fields: BTreeMap::new(),
            module_id: String::new(),
            skill_ids: Vec::new(),
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

    /// Attach normalized tags (caller runs `normalize_tags` first).
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Attach a validated due date (caller runs `parse_due_date` first).
    pub fn with_due_date(mut self, due_date: Option<String>) -> Self {
        self.due_date = due_date;
        self
    }

    /// Attach a parent id (application layer already checked existence/same project).
    pub fn with_parent(mut self, parent_id: Option<String>) -> Self {
        self.parent_id = parent_id;
        self
    }

    /// Attach validated custom fields (caller runs `normalize_custom_fields` first).
    pub fn with_custom_fields(mut self, custom_fields: BTreeMap<String, String>) -> Self {
        self.custom_fields = custom_fields;
        self
    }

    /// Attach a module id (shared module tree; empty = unfiled).
    pub fn with_module(mut self, module_id: &str) -> Self {
        self.module_id = module_id.trim().to_string();
        self
    }

    /// Attach selected project skill ids (trimmed, empties dropped, deduped in order).
    /// The agent runtime composes these into the dispatch prompt at execution time.
    pub fn with_skill_ids(mut self, skill_ids: Vec<String>) -> Self {
        let mut seen = std::collections::HashSet::new();
        self.skill_ids = skill_ids
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && seen.insert(s.clone()))
            .collect();
        self
    }
}

/// Requirement counts grouped by status (project dashboard).
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

/// Requirement priority; P0 is highest, defaults to P2.
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

    /// Match after normalization (trim + uppercase); unknown values → None.
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

/// Parse a priority; invalid values are a validation error (HTTP layer maps it to 400).
pub fn parse_priority(s: &str) -> Result<RequirementPriority, RequirementError> {
    RequirementPriority::parse(s)
        .ok_or_else(|| RequirementError::InvalidPriority(s.trim().to_string()))
}

/// Requirement type; defaults to FEATURE.
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

    /// Match after normalization (trim + uppercase); unknown values → None.
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

/// Parse a requirement type; invalid values are a validation error (HTTP layer maps it to 400).
pub fn parse_req_type(s: &str) -> Result<RequirementType, RequirementError> {
    RequirementType::parse(s).ok_or_else(|| RequirementError::InvalidReqType(s.trim().to_string()))
}

/// Work progress status (shared by dev and test); defaults to not started.
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

    /// Match after normalization (trim + uppercase); unknown values → None.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "NOT_STARTED" => Some(Self::NotStarted),
            "IN_PROGRESS" => Some(Self::InProgress),
            "DONE" => Some(Self::Done),
            _ => None,
        }
    }
}

/// Parse a work status; invalid values are a validation error (HTTP layer maps it to 400).
pub fn parse_work_status(s: &str) -> Result<WorkStatus, RequirementError> {
    WorkStatus::parse(s).ok_or_else(|| RequirementError::InvalidWorkStatus(s.trim().to_string()))
}

/// Requirement pipeline stage; fixed order: created → audit → review → dev → test → acceptance → delivery.
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
    /// All stages, in pipeline order.
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

    /// Match after normalization (trim + uppercase); unknown values → None.
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

/// Parse a stage; invalid values are a validation error (HTTP layer maps it to 400).
pub fn parse_stage(s: &str) -> Result<Stage, RequirementError> {
    Stage::parse(s).ok_or_else(|| RequirementError::InvalidStage(s.trim().to_string()))
}

/// Stage progress status; defaults to pending.
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

    /// Match after normalization (trim + uppercase); unknown values → None.
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

/// Parse a stage status; invalid values are a validation error (HTTP layer maps it to 400).
pub fn parse_stage_status(s: &str) -> Result<StageStatus, RequirementError> {
    StageStatus::parse(s).ok_or_else(|| RequirementError::InvalidStageStatus(s.trim().to_string()))
}

/// One stage row: status + planned start/end dates (`YYYY-MM-DD`) + actual start/finish times (epoch ms).
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
    /// Default row for a stage: PENDING, no plan, no timestamps.
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

    /// Advance the status and stamp times: IN_PROGRESS records the start time;
    /// DONE backfills start/finish. Timestamps are first-write-wins and never
    /// overwritten; going back to PENDING keeps them; SKIPPED is reachable from
    /// any state and stamps nothing.
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

/// Normalize sparse stage rows into the fixed 7 rows (pipeline order, missing
/// rows filled with PENDING defaults); legacy data without backfill still gets
/// a complete pipeline.
pub fn fill_stages(rows: &[StageRow]) -> Vec<StageRow> {
    Stage::ALL
        .iter()
        .map(|s| {
            rows.iter().find(|r| r.stage == *s).cloned().unwrap_or_else(|| StageRow::pending(*s))
        })
        .collect()
}

/// Stage overdue check (computed on read, not persisted): only when a planned
/// end date is set and the stage is not skipped — a finished stage is overdue
/// if it finished after the plan; an unfinished one if today is past the plan
/// (the planned day itself does not count).
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

/// Tag normalization: trim, drop empties, dedupe keeping first occurrence;
/// more than `MAX_TAGS` tags or one longer than `MAX_TAG_LEN` is a validation error.
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

/// Custom field normalization: keys must be non-empty after trim and at most
/// `MAX_CUSTOM_FIELD_KEY_LEN` chars, values at most `MAX_CUSTOM_FIELD_VALUE_LEN`
/// chars, at most `MAX_CUSTOM_FIELDS` keys (all lengths counted in chars).
pub fn normalize_custom_fields(
    raw: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, RequirementError> {
    let mut out = BTreeMap::new();
    for (k, v) in raw {
        let k = k.trim();
        if k.is_empty() {
            return Err(RequirementError::EmptyCustomFieldKey);
        }
        if k.chars().count() > MAX_CUSTOM_FIELD_KEY_LEN {
            return Err(RequirementError::CustomFieldKeyTooLong(k.to_string()));
        }
        if v.chars().count() > MAX_CUSTOM_FIELD_VALUE_LEN {
            return Err(RequirementError::CustomFieldValueTooLong(k.to_string()));
        }
        out.insert(k.to_string(), v.clone());
    }
    if out.len() > MAX_CUSTOM_FIELDS {
        return Err(RequirementError::TooManyCustomFields);
    }
    Ok(out)
}

/// Validate a due date string: strict `YYYY-MM-DD` with valid month/day ranges (leap years included).
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

/// Epoch ms → UTC date string `YYYY-MM-DD` (civil-from-days algorithm; pure function for easy testing).
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

/// Change log: one field-level change to append (timestamp stamped by the storage layer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChange {
    pub changed_by: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

/// Change log: one persisted record (read side).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeEntry {
    pub changed_at_ms: i64,
    pub changed_by: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

/// Immutable snapshot: never rewritten once created; revisions only append new versions.
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
    /// Custom field values, map of field key → string value; definitions are
    /// managed by the project template, multi-select values are comma-joined.
    pub custom_fields: BTreeMap<String, String>,
    /// Module id (shared ms_module project-level module tree); empty = unfiled.
    pub module_id: String,
    /// Selected project skill ids (composed into agent instructions at dispatch time).
    pub skill_ids: Vec<String>,
    /// Due date `YYYY-MM-DD`; overdue is computed on read by `is_overdue`, not persisted.
    pub due_date: Option<String>,
    /// Created/updated timestamps (epoch ms, DB-wide convention); stamped by
    /// the storage layer, 0 means not yet persisted.
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Creator (login user id); may be empty for legacy data.
    pub created_by: String,
    pub dev_status: WorkStatus,
    pub test_status: WorkStatus,
    pub dev_started_at_ms: Option<i64>,
    pub dev_finished_at_ms: Option<i64>,
    pub test_started_at_ms: Option<i64>,
    pub test_finished_at_ms: Option<i64>,
    /// The 7-stage pipeline, always all stages in order (repository fills missing rows via `fill_stages`).
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
            custom_fields: new.custom_fields.clone(),
            module_id: new.module_id.clone(),
            skill_ids: new.skill_ids.clone(),
            due_date: new.due_date.clone(),
            created_at_ms: 0,
            updated_at_ms: 0,
            created_by: new.created_by.clone(),
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

    /// Append a new version without moving the baseline; archived requirements reject revision.
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

    /// Review rejection: record the reason and keep the requirement in DRAFT
    /// for re-review; only valid for drafts pending review.
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

    /// The first baseline promotes Draft → Baselined.
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

    /// Baselined → Delivered, idempotent; not yet baselined → NotBaselined.
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

    /// Soft delete frees the title (a same-titled requirement can be recreated).
    pub fn soft_delete(&mut self) {
        self.deleted = true;
    }

    /// Only non-deleted requirements participate in title uniqueness.
    pub fn occupies_title(&self) -> bool {
        !self.deleted
    }

    /// Overdue check (computed on read, not persisted): the due-date rule (due
    /// date set, past `today`, and not yet delivered/archived) or any stage
    /// overdue (see `stage_overdue`).
    pub fn is_overdue(&self, today: &str) -> bool {
        let due_overdue = match (&self.due_date, self.status) {
            (_, RequirementStatus::Delivered | RequirementStatus::Archived) => false,
            (Some(due), _) => due.as_str() < today,
            (None, _) => false,
        };
        due_overdue || self.stages.iter().any(|row| stage_overdue(row, today))
    }

    /// Current stage: the first one, in order, that is neither done nor skipped; all done → delivery.
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

    /// Advance a stage's status and stamp times; rules in `StageRow::set_status`.
    pub fn set_stage(&mut self, stage: Stage, status: StageStatus, now_ms: i64) {
        if let Some(row) = self.stage_row_mut(stage) {
            row.set_status(status, now_ms);
        }
    }

    /// Advance the dev status and stamp times: the first IN_PROGRESS records
    /// the start time; DONE records the finish time (backfilling the start if
    /// missing); repeats never overwrite existing timestamps.
    pub fn set_dev_status(&mut self, status: WorkStatus, now_ms: i64) {
        self.dev_status = status;
        stamp(status, now_ms, &mut self.dev_started_at_ms, &mut self.dev_finished_at_ms);
    }

    /// Advance the test status; same rules as `set_dev_status`.
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
        // Exactly 10 tags is still legal.
        let ten: Vec<String> = (0..10).map(|i| format!("t{i}")).collect();
        assert_eq!(normalize_tags(&ten).expect("ok").len(), 10);
        let long = "标".repeat(33);
        assert_eq!(
            normalize_tags(std::slice::from_ref(&long)),
            Err(RequirementError::TagTooLong(long))
        );
        // 32 chars (CJK included) is legal.
        assert!(normalize_tags(&["标".repeat(32)]).is_ok());
    }

    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn normalize_custom_fields_trims_keys_and_keeps_values() {
        let raw = fields(&[(" owner ", "alice"), ("模块", "登录")]);
        let out = normalize_custom_fields(&raw).expect("ok");
        assert_eq!(out, fields(&[("owner", "alice"), ("模块", "登录")]));
        assert_eq!(normalize_custom_fields(&BTreeMap::new()), Ok(BTreeMap::new()));
    }

    #[test]
    fn normalize_custom_fields_rejects_blank_or_long_key() {
        assert_eq!(
            normalize_custom_fields(&fields(&[("   ", "v")])),
            Err(RequirementError::EmptyCustomFieldKey)
        );
        let long_key = "键".repeat(65);
        assert_eq!(
            normalize_custom_fields(&fields(&[(long_key.as_str(), "v")])),
            Err(RequirementError::CustomFieldKeyTooLong(long_key.clone()))
        );
        // 64 chars (CJK included) is legal.
        assert!(normalize_custom_fields(&fields(&[("键".repeat(64).as_str(), "v")])).is_ok());
    }

    #[test]
    fn normalize_custom_fields_rejects_long_value_or_too_many_keys() {
        let long_val = "值".repeat(2001);
        assert_eq!(
            normalize_custom_fields(&fields(&[("k", long_val.as_str())])),
            Err(RequirementError::CustomFieldValueTooLong("k".to_string()))
        );
        // Exactly 2000 chars is legal.
        assert!(normalize_custom_fields(&fields(&[("k", "值".repeat(2000).as_str())])).is_ok());
        let many: BTreeMap<String, String> =
            (0..33).map(|i| (format!("k{i}"), "v".to_string())).collect();
        assert_eq!(normalize_custom_fields(&many), Err(RequirementError::TooManyCustomFields));
        let ok: BTreeMap<String, String> =
            (0..32).map(|i| (format!("k{i}"), "v".to_string())).collect();
        assert_eq!(normalize_custom_fields(&ok).expect("ok").len(), 32);
    }

    #[test]
    fn create_carries_custom_fields() {
        let n = new_req().with_custom_fields(fields(&[("owner", "alice")]));
        let r = Requirement::create("req-1", &n);
        assert_eq!(r.custom_fields, fields(&[("owner", "alice")]));
        // Defaults to an empty map.
        assert!(Requirement::create("req-2", &new_req()).custom_fields.is_empty());
    }

    #[test]
    fn parse_due_date_validates_format_and_ranges() {
        assert_eq!(parse_due_date(" 2026-07-11 "), Ok("2026-07-11".to_string()));
        assert_eq!(parse_due_date("2024-02-29"), Ok("2024-02-29".to_string())); // leap year
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
        // 2026-07-11 00:00:00 UTC = 1783728000s.
        assert_eq!(date_from_epoch_ms(1_783_728_000_000), "2026-07-11");
        // One millisecond earlier is still Jul 10.
        assert_eq!(date_from_epoch_ms(1_783_727_999_999), "2026-07-10");
        // Leap day: 2024-02-29 12:00 UTC.
        assert_eq!(date_from_epoch_ms(1_709_208_000_000), "2024-02-29");
    }

    #[test]
    fn overdue_requires_past_due_date_and_open_status() {
        let mut r = Requirement::create("req-1", &new_req());
        assert!(!r.is_overdue("2026-07-11")); // no due date
        r.due_date = Some("2026-07-10".to_string());
        assert!(r.is_overdue("2026-07-11")); // past due
        assert!(!r.is_overdue("2026-07-10")); // the due day itself is not overdue
        assert!(!r.is_overdue("2026-07-09")); // not yet due
        r.status = RequirementStatus::Delivered;
        assert!(!r.is_overdue("2026-07-11")); // delivered is never overdue
        r.status = RequirementStatus::Archived;
        assert!(!r.is_overdue("2026-07-11")); // archived is never overdue
    }

    #[test]
    fn dev_status_stamps_started_and_finished_once() {
        let mut r = Requirement::create("req-1", &new_req());
        assert_eq!(r.dev_status, WorkStatus::NotStarted);
        r.set_dev_status(WorkStatus::InProgress, 100);
        assert_eq!(r.dev_started_at_ms, Some(100));
        assert_eq!(r.dev_finished_at_ms, None);
        // Repeats do not overwrite.
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
    fn create_carries_module_and_defaults_to_unfiled() {
        let r = Requirement::create("req-1", &new_req().with_module(" mod-1 "));
        assert_eq!(r.module_id, "mod-1"); // with_module trims
        assert_eq!(Requirement::create("req-2", &new_req()).module_id, "");
    }

    #[test]
    fn create_carries_skill_ids_trimmed_and_deduped() {
        let n = new_req().with_skill_ids(vec![
            " sk-1 ".to_string(),
            "sk-2".to_string(),
            "sk-1".to_string(),
            "".to_string(),
        ]);
        let r = Requirement::create("req-1", &n);
        assert_eq!(r.skill_ids, vec!["sk-1".to_string(), "sk-2".to_string()]);
        // Defaults to empty.
        assert!(Requirement::create("req-2", &new_req()).skill_ids.is_empty());
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
        // Empty input → 7 PENDING rows in pipeline order.
        let filled = fill_stages(&[]);
        assert_eq!(filled.len(), 7);
        assert_eq!(filled.iter().map(|r| r.stage).collect::<Vec<_>>(), Stage::ALL);
        assert!(filled.iter().all(|r| r.status == StageStatus::Pending));

        // Sparse out-of-order input: existing rows kept, missing rows defaulted.
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

    // 2026-07-11 00:00:00 UTC = 1_783_728_000_000 ms (see the date_from_epoch_ms test).
    const JUL_11_MS: i64 = 1_783_728_000_000;

    #[test]
    fn stage_overdue_matrix() {
        let today = "2026-07-11";
        let mut row = StageRow::pending(Stage::Dev);
        // No planned end date → not overdue.
        assert!(!stage_overdue(&row, today));

        // Unfinished and today is past the plan (the planned day itself does not count).
        row.planned_end = Some("2026-07-10".to_string());
        assert!(stage_overdue(&row, today));
        assert!(!stage_overdue(&row, "2026-07-10"));
        assert!(!stage_overdue(&row, "2026-07-09"));

        // Skipped stages are never overdue.
        row.set_status(StageStatus::Skipped, JUL_11_MS);
        assert!(stage_overdue(&StageRow { status: StageStatus::InProgress, ..row.clone() }, today));
        assert!(!stage_overdue(&row, today));

        // Finished on time: finish date <= plan → not overdue even if today is past the plan.
        let mut done = StageRow::pending(Stage::Test);
        done.planned_end = Some("2026-07-10".to_string());
        done.set_status(StageStatus::Done, JUL_11_MS - 86_400_000); // finished on 07-10
        assert!(!stage_overdue(&done, "2026-08-01"));

        // Finished late: finish date > plan → permanently overdue, regardless of today.
        let mut late = StageRow::pending(Stage::Test);
        late.planned_end = Some("2026-07-10".to_string());
        late.set_status(StageStatus::Done, JUL_11_MS); // finished on 07-11
        assert!(stage_overdue(&late, "2026-07-01"));

        // DONE but finish time missing (should not happen) → not misjudged as unfinished.
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
        // A stage being overdue alone (no due date) makes the requirement overdue.
        r.stage_row_mut(Stage::Dev).expect("dev row").planned_end = Some("2026-07-01".to_string());
        assert!(r.is_overdue("2026-07-11"));
        // Delivered exempts the due-date rule, but a late stage still counts.
        r.status = RequirementStatus::Delivered;
        assert!(r.is_overdue("2026-07-11"));
        // No longer overdue once the stage finishes on time.
        r.set_stage(Stage::Dev, StageStatus::Done, 1_751_328_000_000); // finished 2025-07-01, before the plan
        assert!(!r.is_overdue("2026-07-11"));
    }

    #[test]
    fn current_stage_walks_pipeline_in_order() {
        let mut r = Requirement::create("req-1", &new_req());
        assert_eq!(r.current_stage(), Stage::Created);
        r.set_stage(Stage::Created, StageStatus::Done, 1);
        assert_eq!(r.current_stage(), Stage::Audit);
        // SKIPPED counts as passed.
        r.set_stage(Stage::Audit, StageStatus::Skipped, 2);
        assert_eq!(r.current_stage(), Stage::Review);
        // IN_PROGRESS stays at the current stage.
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
        // Repeats do not overwrite.
        r.set_stage(Stage::Dev, StageStatus::InProgress, 200);
        assert_eq!(dev(&r).started_at_ms, Some(100));
        // Moving back to PENDING keeps the timestamps.
        r.set_stage(Stage::Dev, StageStatus::Pending, 300);
        assert_eq!(dev(&r).status, StageStatus::Pending);
        assert_eq!(dev(&r).started_at_ms, Some(100));
        r.set_stage(Stage::Dev, StageStatus::Done, 400);
        assert_eq!(dev(&r).started_at_ms, Some(100));
        assert_eq!(dev(&r).finished_at_ms, Some(400));
        r.set_stage(Stage::Dev, StageStatus::Done, 500);
        assert_eq!(dev(&r).finished_at_ms, Some(400));
        // SKIPPED is reachable even after DONE (any state allowed); timestamps kept.
        r.set_stage(Stage::Dev, StageStatus::Skipped, 600);
        assert_eq!(dev(&r).status, StageStatus::Skipped);
        assert_eq!(dev(&r).finished_at_ms, Some(400));
        // Going straight to DONE backfills the start time.
        r.set_stage(Stage::Test, StageStatus::Done, 700);
        let test = r.stage_row(Stage::Test).expect("test row");
        assert_eq!(test.started_at_ms, Some(700));
        assert_eq!(test.finished_at_ms, Some(700));
    }
}
