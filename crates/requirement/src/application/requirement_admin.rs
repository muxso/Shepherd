use std::collections::BTreeMap;
use std::sync::Arc;

use crate::domain::{
    normalize_custom_fields, normalize_tags, parse_criteria, parse_due_date, parse_priority,
    parse_req_type, parse_stage, parse_stage_status, ChangeEntry, NewChange, Requirement,
    RequirementError, Stage, StageStatus, StatusCounts,
};
use crate::ports::{RepoError, RequirementRepository};

/// Max change-log entries returned per query.
const MAX_CHANGE_ENTRIES: u32 = 200;
/// Max hops when walking the parent chain (guards against pre-existing cycles in dirty data).
const MAX_ANCESTOR_HOPS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequirementCmdError {
    Validation(RequirementError),
    TitleExists,
    NotFound,
    NoSuchVersion(u32),
    Archived,
    NotUnderReview,
    /// Parent requirement does not exist (or is soft-deleted).
    ParentNotFound,
    /// A requirement cannot be its own parent.
    SelfParent,
    /// Parent must belong to the same project.
    CrossProjectParent,
    /// Setting this parent would create a cycle.
    ParentCycle,
    Repo(RepoError),
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Optional value as a change-log string (None → empty).
fn opt_str(v: Option<&str>) -> String {
    v.unwrap_or_default().to_string()
}

/// Planned-date input: outer None keeps current; `Some(None)` clears; `Some(Some(d))` must be YYYY-MM-DD.
fn parse_planned(v: Option<Option<&str>>) -> Result<Option<Option<String>>, RequirementError> {
    match v {
        None => Ok(None),
        Some(None) => Ok(Some(None)),
        Some(Some(d)) => Ok(Some(Some(parse_due_date(d)?))),
    }
}

fn change(by: &str, field: &str, old: impl Into<String>, new: impl Into<String>) -> NewChange {
    NewChange {
        changed_by: by.to_string(),
        field: field.to_string(),
        old_value: old.into(),
        new_value: new.into(),
    }
}

impl From<RepoError> for RequirementCmdError {
    fn from(e: RepoError) -> Self {
        Self::Repo(e)
    }
}

impl From<RequirementError> for RequirementCmdError {
    fn from(e: RequirementError) -> Self {
        match e {
            RequirementError::NoSuchVersion(n) => Self::NoSuchVersion(n),
            RequirementError::Archived => Self::Archived,
            RequirementError::NotUnderReview => Self::NotUnderReview,
            other => Self::Validation(other),
        }
    }
}

#[derive(Clone)]
pub struct RequirementService {
    repo: Arc<dyn RequirementRepository>,
}

impl RequirementService {
    pub fn new(repo: Arc<dyn RequirementRepository>) -> Self {
        Self { repo }
    }

    /// Soft-deleted requirements are treated as missing.
    pub async fn get(&self, id: &str) -> Result<Requirement, RequirementCmdError> {
        self.repo.get(id).await?.filter(|r| !r.deleted).ok_or(RequirementCmdError::NotFound)
    }

    /// Manual ordering: writes explicit ranks for these requirements in the given order.
    pub async fn reorder(
        &self,
        project_id: &str,
        ordered_ids: &[String],
    ) -> Result<(), RequirementCmdError> {
        self.repo.set_order(project_id, ordered_ids).await?;
        Ok(())
    }

    /// Per-status aggregation of a project's requirements (dashboard).
    pub async fn status_summary(
        &self,
        project_id: &str,
    ) -> Result<StatusCounts, RequirementCmdError> {
        Ok(self.repo.status_counts(project_id).await?)
    }

    pub async fn revise(
        &self,
        id: &str,
        description: &str,
        criteria: &[String],
        by: &str,
    ) -> Result<u32, RequirementCmdError> {
        let mut req = self.get(id).await?;
        let parsed = parse_criteria(criteria)?;
        let old = req.latest_version();
        let version = req.revise(description, parsed)?;
        self.repo.save(&req).await?;
        self.repo
            .append_change(id, &[change(by, "version", old.to_string(), version.to_string())])
            .await?;
        Ok(version)
    }

    pub async fn set_baseline(
        &self,
        id: &str,
        version: u32,
        by: &str,
    ) -> Result<Requirement, RequirementCmdError> {
        let mut req = self.get(id).await?;
        let old = req.status;
        req.set_baseline(version)?;
        self.repo.save(&req).await?;
        if old != req.status {
            self.repo
                .append_change(id, &[change(by, "status", old.as_str(), req.status.as_str())])
                .await?;
        }
        // Baselining implies the review passed.
        self.stage_hook(&mut req, Stage::Review, StageStatus::Done, by).await;
        Ok(req)
    }

    /// Title uniqueness ignores soft-deleted rows and the requirement itself.
    pub async fn rename(
        &self,
        id: &str,
        title: &str,
        by: &str,
    ) -> Result<Requirement, RequirementCmdError> {
        self.update(id, title, None, None, None, None, None, None, by).await
    }

    /// Rename plus optional updates to priority/type/tags/due date/custom fields/module:
    /// None keeps the current value, invalid values fail validation.
    /// `due_date`: outer None keeps; `Some(None)` clears; `Some(Some(d))` sets d (must be YYYY-MM-DD).
    /// `custom_fields`: None keeps; Some(map) replaces wholesale (empty map clears).
    /// `module_id`: None keeps; Some replaces (empty string = unfiled).
    #[allow(clippy::too_many_arguments)]
    pub async fn update(
        &self,
        id: &str,
        title: &str,
        priority: Option<&str>,
        req_type: Option<&str>,
        tags: Option<&[String]>,
        due_date: Option<Option<&str>>,
        custom_fields: Option<&BTreeMap<String, String>>,
        module_id: Option<&str>,
        by: &str,
    ) -> Result<Requirement, RequirementCmdError> {
        let priority = priority.map(parse_priority).transpose()?;
        let req_type = req_type.map(parse_req_type).transpose()?;
        let tags = tags.map(normalize_tags).transpose()?;
        let custom_fields = custom_fields.map(normalize_custom_fields).transpose()?;
        let due_date = match due_date {
            None => None,
            Some(None) => Some(None),
            Some(Some(d)) => Some(Some(parse_due_date(d)?)),
        };
        let mut req = self.get(id).await?;
        let trimmed = title.trim();
        if let Some(existing) = self.repo.find_active_by_title(&req.project_id, trimmed).await? {
            if existing.id != req.id {
                return Err(RequirementCmdError::TitleExists);
            }
        }
        let mut log = Vec::new();
        if trimmed != req.title {
            log.push(change(by, "title", req.title.clone(), trimmed));
        }
        req.rename(title)?;
        if let Some(p) = priority {
            if p != req.priority {
                log.push(change(by, "priority", req.priority.as_str(), p.as_str()));
            }
            req.priority = p;
        }
        if let Some(t) = req_type {
            if t != req.req_type {
                log.push(change(by, "reqType", req.req_type.as_str(), t.as_str()));
            }
            req.req_type = t;
        }
        if let Some(t) = tags {
            if t != req.tags {
                log.push(change(by, "tags", req.tags.join(","), t.join(",")));
            }
            req.tags = t;
        }
        if let Some(d) = due_date {
            if d != req.due_date {
                log.push(change(
                    by,
                    "dueDate",
                    opt_str(req.due_date.as_deref()),
                    opt_str(d.as_deref()),
                ));
            }
            req.due_date = d;
        }
        if let Some(cf) = custom_fields {
            // Wholesale replace; diff per key for the change log: added keys log empty old, removed keys empty new.
            for (k, old_v) in &req.custom_fields {
                match cf.get(k) {
                    Some(new_v) if new_v != old_v => {
                        log.push(change(by, &format!("custom.{k}"), old_v.clone(), new_v.clone()));
                    }
                    Some(_) => {}
                    None => log.push(change(by, &format!("custom.{k}"), old_v.clone(), "")),
                }
            }
            for (k, new_v) in &cf {
                if !req.custom_fields.contains_key(k) {
                    log.push(change(by, &format!("custom.{k}"), "", new_v.clone()));
                }
            }
            req.custom_fields = cf;
        }
        if let Some(m) = module_id {
            let m = m.trim();
            if m != req.module_id {
                log.push(change(by, "module", req.module_id.clone(), m));
            }
            req.module_id = m.to_string();
        }
        self.repo.save(&req).await?;
        if !log.is_empty() {
            self.repo.append_change(id, &log).await?;
        }
        Ok(req)
    }

    /// Advance a pipeline stage: optionally change status (stamping rules in `StageRow::set_status`)
    /// and optionally set/clear planned dates (outer None keeps; `Some(None)` clears;
    /// `Some(Some(d))` sets d, must be YYYY-MM-DD).
    pub async fn set_stage(
        &self,
        id: &str,
        stage: &str,
        status: Option<&str>,
        planned_start: Option<Option<&str>>,
        planned_end: Option<Option<&str>>,
        by: &str,
    ) -> Result<Requirement, RequirementCmdError> {
        let stage = parse_stage(stage)?;
        let status = status.map(parse_stage_status).transpose()?;
        let planned_start = parse_planned(planned_start)?;
        let planned_end = parse_planned(planned_end)?;

        let mut req = self.get(id).await?;
        let field = format!("stage.{}", stage.as_str());
        let mut log = Vec::new();
        {
            let row = req
                .stage_row_mut(stage)
                .expect("stages always contain all pipeline stages after repo fill");
            if let Some(s) = status {
                if s != row.status {
                    log.push(change(by, &field, row.status.as_str(), s.as_str()));
                }
                row.set_status(s, now_ms());
            }
            if let Some(d) = planned_start {
                if d != row.planned_start {
                    log.push(change(
                        by,
                        &format!("{field}.plannedStart"),
                        opt_str(row.planned_start.as_deref()),
                        opt_str(d.as_deref()),
                    ));
                }
                row.planned_start = d;
            }
            if let Some(d) = planned_end {
                if d != row.planned_end {
                    log.push(change(
                        by,
                        &format!("{field}.plannedEnd"),
                        opt_str(row.planned_end.as_deref()),
                        opt_str(d.as_deref()),
                    ));
                }
                row.planned_end = d;
            }
        }
        let row =
            req.stage_row(stage).expect("stage row just mutated must still be present").clone();
        self.repo.upsert_stage(id, &row).await?;
        if !log.is_empty() {
            self.repo.append_change(id, &log).await?;
        }
        Ok(req)
    }

    /// Stage auto-hook: syncs the matching stage row after a business action succeeds;
    /// skipped when already at the target status. A failed stage write must not fail the
    /// main operation — warn only (reads fall back to `fill_stages` default rows).
    async fn stage_hook(&self, req: &mut Requirement, stage: Stage, status: StageStatus, by: &str) {
        let Some(row) = req.stage_row(stage) else { return };
        let old = row.status;
        if old == status {
            return;
        }
        let mut updated = row.clone();
        updated.set_status(status, now_ms());
        if let Err(e) = self.repo.upsert_stage(&req.id, &updated).await {
            tracing::warn!(requirement = %req.id, stage = stage.as_str(), error = %e, "阶段钩子写入失败");
            return;
        }
        if let Some(slot) = req.stage_row_mut(stage) {
            *slot = updated;
        }
        let entry = change(by, &format!("stage.{}", stage.as_str()), old.as_str(), status.as_str());
        if let Err(e) = self.repo.append_change(&req.id, &[entry]).await {
            tracing::warn!(requirement = %req.id, stage = stage.as_str(), error = %e, "阶段变更日志写入失败");
        }
    }

    /// Attach/detach parent: parent must exist (not soft-deleted), be in the same project,
    /// not be the requirement itself, and must not create a cycle.
    pub async fn set_parent(
        &self,
        id: &str,
        parent_id: Option<&str>,
        by: &str,
    ) -> Result<Requirement, RequirementCmdError> {
        let mut req = self.get(id).await?;
        let new_parent = match parent_id.map(str::trim).filter(|p| !p.is_empty()) {
            None => None,
            Some(p) => {
                self.validate_parent(&req, p).await?;
                Some(p.to_string())
            }
        };
        if new_parent == req.parent_id {
            return Ok(req);
        }
        let old = req.parent_id.take();
        req.parent_id = new_parent;
        self.repo.save(&req).await?;
        self.repo
            .append_change(
                id,
                &[change(by, "parent", opt_str(old.as_deref()), opt_str(req.parent_id.as_deref()))],
            )
            .await?;
        Ok(req)
    }

    async fn validate_parent(
        &self,
        req: &Requirement,
        parent_id: &str,
    ) -> Result<(), RequirementCmdError> {
        if parent_id == req.id {
            return Err(RequirementCmdError::SelfParent);
        }
        let parent = self
            .repo
            .get(parent_id)
            .await?
            .filter(|p| !p.deleted)
            .ok_or(RequirementCmdError::ParentNotFound)?;
        if parent.project_id != req.project_id {
            return Err(RequirementCmdError::CrossProjectParent);
        }
        // Walk up the parent chain: hitting self means a cycle; hops are capped to guard
        // against pre-existing cycles in dirty data.
        let mut cursor = parent.parent_id;
        for _ in 0..MAX_ANCESTOR_HOPS {
            let Some(ancestor_id) = cursor else { return Ok(()) };
            if ancestor_id == req.id {
                return Err(RequirementCmdError::ParentCycle);
            }
            cursor = self.repo.get(&ancestor_id).await?.and_then(|a| a.parent_id);
        }
        Ok(())
    }

    /// Direct children (not soft-deleted).
    pub async fn children(&self, id: &str) -> Result<Vec<Requirement>, RequirementCmdError> {
        self.get(id).await?;
        Ok(self.repo.children(id).await?)
    }

    /// Change log, newest first, capped at `MAX_CHANGE_ENTRIES`.
    pub async fn changes(&self, id: &str) -> Result<Vec<ChangeEntry>, RequirementCmdError> {
        self.get(id).await?;
        Ok(self.repo.list_changes(id, MAX_CHANGE_ENTRIES).await?)
    }

    // TODO(AI review): beyond manual approve/reject, add an AI review opinion — retrieve the
    // requirement's linked test cases (functional coverage chain) and PRD corpus (RAG) and have
    // the model output "approve/reject + rationale" as decision input. Lands as the fifth LLM
    // touchpoint (see server/llm.rs for the first four: split/execute/verify/case drafting);
    // the review gate stays human-decided, with the AI verdict shown as an attached opinion.
    pub async fn reject_review(
        &self,
        id: &str,
        reason: &str,
        by: &str,
    ) -> Result<Requirement, RequirementCmdError> {
        let mut req = self.get(id).await?;
        let old = req.status;
        req.reject_review(reason)?;
        self.repo.save(&req).await?;
        // Rejection keeps the status (stays DRAFT for re-review) but still logs a status entry
        // to record the review action.
        self.repo
            .append_change(id, &[change(by, "status", old.as_str(), req.status.as_str())])
            .await?;
        // Rejection means the review is still in progress.
        self.stage_hook(&mut req, Stage::Review, StageStatus::InProgress, by).await;
        Ok(req)
    }

    pub async fn deliver(&self, id: &str, by: &str) -> Result<Requirement, RequirementCmdError> {
        let mut req = self.get(id).await?;
        let old = req.status;
        req.deliver()?;
        self.repo.save(&req).await?;
        if old != req.status {
            self.repo
                .append_change(id, &[change(by, "status", old.as_str(), req.status.as_str())])
                .await?;
        }
        // Delivering completes acceptance (if not already) and delivery.
        self.stage_hook(&mut req, Stage::Acceptance, StageStatus::Done, by).await;
        self.stage_hook(&mut req, Stage::Delivery, StageStatus::Done, by).await;
        Ok(req)
    }

    pub async fn archive(&self, id: &str, by: &str) -> Result<Requirement, RequirementCmdError> {
        let mut req = self.get(id).await?;
        let old = req.status;
        req.archive();
        self.repo.save(&req).await?;
        if old != req.status {
            self.repo
                .append_change(id, &[change(by, "status", old.as_str(), req.status.as_str())])
                .await?;
        }
        Ok(req)
    }

    pub async fn delete(&self, id: &str) -> Result<(), RequirementCmdError> {
        let mut req = self.get(id).await?;
        req.soft_delete();
        self.repo.save(&req).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryRequirementRepository;
    use crate::application::CreateRequirementUseCase;

    async fn seeded() -> (RequirementService, String) {
        let repo = Arc::new(InMemoryRequirementRepository::new());
        let id = CreateRequirementUseCase::new(repo.clone())
            .execute("p1", "登录", "d", &["c1".to_string()])
            .await
            .expect("seed")
            .id;
        (RequirementService::new(repo), id)
    }

    #[tokio::test]
    async fn revise_then_set_baseline_persists() {
        let (svc, id) = seeded().await;
        let v = svc.revise(&id, "v2", &["c2".to_string()], "u1").await.expect("revise");
        assert_eq!(v, 2);
        assert_eq!(svc.get(&id).await.expect("get").baseline_version, 1);
        let r = svc.set_baseline(&id, 2, "u1").await.expect("baseline");
        assert_eq!(r.baseline_version, 2);
        assert_eq!(svc.get(&id).await.expect("get").baseline_version, 2);
    }

    #[tokio::test]
    async fn reject_review_persists_reason_and_baseline_clears_it() {
        let (svc, id) = seeded().await;
        let r = svc.reject_review(&id, "  缺少异常路径  ", "u1").await.expect("reject");
        assert_eq!(r.review_comment.as_deref(), Some("缺少异常路径"));
        assert_eq!(
            svc.get(&id).await.expect("get").review_comment.as_deref(),
            Some("缺少异常路径")
        );
        let p = svc.set_baseline(&id, 1, "u1").await.expect("baseline");
        assert!(p.review_comment.is_none());
    }

    #[tokio::test]
    async fn reject_review_empty_reason_is_validation() {
        let (svc, id) = seeded().await;
        assert_eq!(
            svc.reject_review(&id, "   ", "u1").await.unwrap_err(),
            RequirementCmdError::Validation(crate::domain::RequirementError::EmptyReviewComment)
        );
    }

    #[tokio::test]
    async fn reject_review_on_baselined_is_conflict() {
        let (svc, id) = seeded().await;
        svc.set_baseline(&id, 1, "u1").await.expect("baseline");
        assert_eq!(
            svc.reject_review(&id, "x", "u1").await.unwrap_err(),
            RequirementCmdError::NotUnderReview
        );
    }

    #[tokio::test]
    async fn set_baseline_unknown_version_404() {
        let (svc, id) = seeded().await;
        assert_eq!(
            svc.set_baseline(&id, 9, "u1").await.unwrap_err(),
            RequirementCmdError::NoSuchVersion(9)
        );
    }

    #[tokio::test]
    async fn revise_archived_is_conflict() {
        let (svc, id) = seeded().await;
        svc.archive(&id, "u1").await.expect("archive");
        assert_eq!(
            svc.revise(&id, "v2", &[], "u1").await.unwrap_err(),
            RequirementCmdError::Archived
        );
    }

    #[tokio::test]
    async fn rename_to_taken_title_conflicts() {
        let repo = Arc::new(InMemoryRequirementRepository::new());
        let create = CreateRequirementUseCase::new(repo.clone());
        let a = create.execute("p1", "登录", "d", &[]).await.expect("a").id;
        create.execute("p1", "注册", "d", &[]).await.expect("b");
        let svc = RequirementService::new(repo);
        assert_eq!(
            svc.rename(&a, "注册", "u1").await.unwrap_err(),
            RequirementCmdError::TitleExists
        );
        assert!(svc.rename(&a, "登入", "u1").await.is_ok());
    }

    #[tokio::test]
    async fn update_sets_priority_and_type_and_none_keeps_them() {
        use crate::domain::{RequirementPriority, RequirementType};
        let (svc, id) = seeded().await;
        let r = svc
            .update(&id, "登录", Some(" p1 "), Some("bugfix"), None, None, None, None, "u1")
            .await
            .expect("update");
        assert_eq!(r.priority, RequirementPriority::P1);
        assert_eq!(r.req_type, RequirementType::Bugfix);
        // None keeps existing values.
        let r2 = svc
            .update(&id, "登入", None, None, None, None, None, None, "u1")
            .await
            .expect("update");
        assert_eq!(r2.title, "登入");
        assert_eq!(r2.priority, RequirementPriority::P1);
        assert_eq!(r2.req_type, RequirementType::Bugfix);
        // Invalid value fails validation.
        assert_eq!(
            svc.update(&id, "登入", Some("P9"), None, None, None, None, None, "u1")
                .await
                .unwrap_err(),
            RequirementCmdError::Validation(RequirementError::InvalidPriority("P9".into()))
        );
    }

    #[tokio::test]
    async fn delete_then_not_found_and_title_freed() {
        let (svc, id) = seeded().await;
        svc.delete(&id).await.expect("delete");
        assert_eq!(svc.get(&id).await.unwrap_err(), RequirementCmdError::NotFound);
    }

    #[tokio::test]
    async fn get_missing_is_not_found() {
        let (svc, _id) = seeded().await;
        assert_eq!(svc.get("ghost").await.unwrap_err(), RequirementCmdError::NotFound);
    }

    fn stage_of(r: &Requirement, stage: Stage) -> crate::domain::StageRow {
        r.stage_row(stage).expect("stages always filled").clone()
    }

    #[tokio::test]
    async fn create_hook_completes_created_stage_with_created_at_stamps() {
        let (svc, id) = seeded().await;
        let r = svc.get(&id).await.expect("get");
        let created = stage_of(&r, Stage::Created);
        assert_eq!(created.status, StageStatus::Done);
        assert_eq!(created.started_at_ms, Some(r.created_at_ms));
        assert_eq!(created.finished_at_ms, Some(r.created_at_ms));
        assert_eq!(r.current_stage(), Stage::Audit);
        // The hook recorded a change-log entry.
        let log = svc.changes(&id).await.expect("changes");
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].field, "stage.CREATED");
        assert_eq!(log[0].old_value, "PENDING");
        assert_eq!(log[0].new_value, "DONE");
    }

    #[tokio::test]
    async fn set_stage_transitions_stamp_first_write_wins_and_persist() {
        let (svc, id) = seeded().await;
        let r = svc
            .set_stage(&id, " dev ", Some(" in_progress "), None, None, "u1")
            .await
            .expect("stage");
        let dev = stage_of(&r, Stage::Dev);
        assert_eq!(dev.status, StageStatus::InProgress);
        let started = dev.started_at_ms.expect("started stamped");
        assert!(dev.finished_at_ms.is_none());
        // Repeated transition does not overwrite the start stamp.
        let r2 =
            svc.set_stage(&id, "DEV", Some("IN_PROGRESS"), None, None, "u1").await.expect("stage");
        assert_eq!(stage_of(&r2, Stage::Dev).started_at_ms, Some(started));
        let r3 = svc.set_stage(&id, "DEV", Some("DONE"), None, None, "u1").await.expect("stage");
        let dev3 = stage_of(&r3, Stage::Dev);
        assert_eq!(dev3.started_at_ms, Some(started));
        let finished = dev3.finished_at_ms.expect("finished stamped");
        let r4 = svc.set_stage(&id, "DEV", Some("DONE"), None, None, "u1").await.expect("stage");
        assert_eq!(stage_of(&r4, Stage::Dev).finished_at_ms, Some(finished));
        // Persisted (survives reload); jumping straight to DONE backfills the start stamp.
        let got = svc.get(&id).await.expect("get");
        assert_eq!(stage_of(&got, Stage::Dev).status, StageStatus::Done);
        let r5 = svc.set_stage(&id, "TEST", Some("done"), None, None, "u1").await.expect("stage");
        let test = stage_of(&r5, Stage::Test);
        assert!(test.started_at_ms.is_some());
        assert!(test.finished_at_ms.is_some());
        // Invalid stage/status fails validation.
        assert_eq!(
            svc.set_stage(&id, "DESIGN", Some("DONE"), None, None, "u1").await.unwrap_err(),
            RequirementCmdError::Validation(RequirementError::InvalidStage("DESIGN".into()))
        );
        assert_eq!(
            svc.set_stage(&id, "DEV", Some("PAUSED"), None, None, "u1").await.unwrap_err(),
            RequirementCmdError::Validation(RequirementError::InvalidStageStatus("PAUSED".into()))
        );
    }

    #[tokio::test]
    async fn set_stage_planned_dates_set_clear_and_log() {
        let (svc, id) = seeded().await;
        let r = svc
            .set_stage(&id, "DEV", None, Some(Some("2026-07-01")), Some(Some("2026-07-31")), "u1")
            .await
            .expect("stage");
        let dev = stage_of(&r, Stage::Dev);
        assert_eq!(dev.planned_start.as_deref(), Some("2026-07-01"));
        assert_eq!(dev.planned_end.as_deref(), Some("2026-07-31"));
        assert_eq!(dev.status, StageStatus::Pending); // plan-only update keeps status
                                                      // Outer None keeps.
        let r2 = svc.set_stage(&id, "DEV", None, None, None, "u1").await.expect("stage");
        assert_eq!(stage_of(&r2, Stage::Dev).planned_end.as_deref(), Some("2026-07-31"));
        // Some(None) clears.
        let r3 = svc.set_stage(&id, "DEV", None, None, Some(None), "u1").await.expect("stage");
        assert!(stage_of(&r3, Stage::Dev).planned_end.is_none());
        assert_eq!(stage_of(&r3, Stage::Dev).planned_start.as_deref(), Some("2026-07-01"));
        // Invalid date fails validation.
        assert_eq!(
            svc.set_stage(&id, "DEV", None, None, Some(Some("2026/07/31")), "u1")
                .await
                .unwrap_err(),
            RequirementCmdError::Validation(RequirementError::InvalidDueDate("2026/07/31".into()))
        );
        // Change log, newest first: clear plannedEnd → set plannedEnd → set plannedStart → creation.
        let log = svc.changes(&id).await.expect("changes");
        let fields: Vec<&str> = log.iter().map(|c| c.field.as_str()).collect();
        assert_eq!(
            fields,
            [
                "stage.DEV.plannedEnd",
                "stage.DEV.plannedEnd",
                "stage.DEV.plannedStart",
                "stage.CREATED"
            ]
        );
        assert_eq!(log[0].old_value, "2026-07-31");
        assert_eq!(log[0].new_value, "");
    }

    #[tokio::test]
    async fn baseline_and_reject_hooks_drive_review_stage() {
        let (svc, id) = seeded().await;
        // Reject → review in progress.
        let r = svc.reject_review(&id, "缺少异常路径", "u1").await.expect("reject");
        assert_eq!(stage_of(&r, Stage::Review).status, StageStatus::InProgress);
        // Baseline → review done.
        let r2 = svc.set_baseline(&id, 1, "u1").await.expect("baseline");
        assert_eq!(stage_of(&r2, Stage::Review).status, StageStatus::Done);
        // Persisted + change log.
        let got = svc.get(&id).await.expect("get");
        assert_eq!(stage_of(&got, Stage::Review).status, StageStatus::Done);
        let fields: Vec<String> =
            svc.changes(&id).await.expect("changes").iter().map(|c| c.field.clone()).collect();
        assert_eq!(fields, ["stage.REVIEW", "status", "stage.REVIEW", "status", "stage.CREATED"]);
    }

    #[tokio::test]
    async fn deliver_hook_completes_acceptance_and_delivery() {
        let (svc, id) = seeded().await;
        svc.set_baseline(&id, 1, "u1").await.expect("baseline");
        let r = svc.deliver(&id, "u1").await.expect("deliver");
        assert_eq!(stage_of(&r, Stage::Acceptance).status, StageStatus::Done);
        assert_eq!(stage_of(&r, Stage::Delivery).status, StageStatus::Done);
        // Repeated delivery is idempotent: no extra stage log entries.
        let n = svc.changes(&id).await.expect("changes").len();
        svc.deliver(&id, "u1").await.expect("deliver again");
        assert_eq!(svc.changes(&id).await.expect("changes").len(), n);
    }

    #[tokio::test]
    async fn deliver_hook_skips_acceptance_already_done() {
        let (svc, id) = seeded().await;
        svc.set_baseline(&id, 1, "u1").await.expect("baseline");
        svc.set_stage(&id, "ACCEPTANCE", Some("DONE"), None, None, "u1").await.expect("stage");
        let finished =
            stage_of(&svc.get(&id).await.expect("get"), Stage::Acceptance).finished_at_ms;
        svc.deliver(&id, "u1").await.expect("deliver");
        // The hook leaves already-done acceptance untouched (timestamp kept) and logs only DELIVERY.
        let got = svc.get(&id).await.expect("get");
        assert_eq!(stage_of(&got, Stage::Acceptance).finished_at_ms, finished);
        let fields: Vec<String> =
            svc.changes(&id).await.expect("changes").iter().map(|c| c.field.clone()).collect();
        assert_eq!(
            fields,
            [
                "stage.DELIVERY",
                "status",
                "stage.ACCEPTANCE",
                "stage.REVIEW",
                "status",
                "stage.CREATED"
            ]
        );
    }

    async fn seeded_many(titles: &[&str]) -> (RequirementService, Vec<String>) {
        let repo = Arc::new(InMemoryRequirementRepository::new());
        let create = CreateRequirementUseCase::new(repo.clone());
        let mut ids = Vec::new();
        for t in titles {
            ids.push(create.execute("p1", t, "d", &[]).await.expect("seed").id);
        }
        (RequirementService::new(repo), ids)
    }

    #[tokio::test]
    async fn set_parent_links_and_children_lists() {
        let (svc, ids) = seeded_many(&["根", "叶A", "叶B"]).await;
        svc.set_parent(&ids[1], Some(&ids[0]), "u1").await.expect("link");
        svc.set_parent(&ids[2], Some(&ids[0]), "u1").await.expect("link");
        let kids = svc.children(&ids[0]).await.expect("children");
        assert_eq!(kids.len(), 2);
        assert!(kids.iter().all(|k| k.parent_id.as_deref() == Some(ids[0].as_str())));
        let r = svc.set_parent(&ids[1], None, "u1").await.expect("unlink");
        assert!(r.parent_id.is_none());
        assert_eq!(svc.children(&ids[0]).await.expect("children").len(), 1);
    }

    #[tokio::test]
    async fn set_parent_rejects_self_missing_cross_project_and_cycle() {
        let repo = Arc::new(InMemoryRequirementRepository::new());
        let create = CreateRequirementUseCase::new(repo.clone());
        let a = create.execute("p1", "A", "d", &[]).await.expect("a").id;
        let b = create.execute("p1", "B", "d", &[]).await.expect("b").id;
        let c = create.execute("p1", "C", "d", &[]).await.expect("c").id;
        let other = create.execute("p2", "X", "d", &[]).await.expect("x").id;
        let svc = RequirementService::new(repo);

        assert_eq!(
            svc.set_parent(&a, Some(&a), "u1").await.unwrap_err(),
            RequirementCmdError::SelfParent
        );
        assert_eq!(
            svc.set_parent(&a, Some("ghost"), "u1").await.unwrap_err(),
            RequirementCmdError::ParentNotFound
        );
        assert_eq!(
            svc.set_parent(&a, Some(&other), "u1").await.unwrap_err(),
            RequirementCmdError::CrossProjectParent
        );
        // With B under A and C under B, hanging A under C forms a cycle.
        svc.set_parent(&b, Some(&a), "u1").await.expect("b under a");
        svc.set_parent(&c, Some(&b), "u1").await.expect("c under b");
        assert_eq!(
            svc.set_parent(&a, Some(&c), "u1").await.unwrap_err(),
            RequirementCmdError::ParentCycle
        );
        // Direct parent-child cycle is rejected too.
        assert_eq!(
            svc.set_parent(&a, Some(&b), "u1").await.unwrap_err(),
            RequirementCmdError::ParentCycle
        );
    }

    #[tokio::test]
    async fn update_sets_tags_and_due_date_with_clear_semantics() {
        let (svc, id) = seeded().await;
        let tags = vec![" api ".to_string(), "web".to_string(), "api".to_string()];
        let r = svc
            .update(
                &id,
                "登录",
                None,
                None,
                Some(&tags),
                Some(Some("2026-12-31")),
                None,
                None,
                "u1",
            )
            .await
            .expect("update");
        assert_eq!(r.tags, vec!["api".to_string(), "web".to_string()]);
        assert_eq!(r.due_date.as_deref(), Some("2026-12-31"));
        // Outer None keeps.
        let r2 = svc
            .update(&id, "登录", None, None, None, None, None, None, "u1")
            .await
            .expect("update");
        assert_eq!(r2.tags, vec!["api".to_string(), "web".to_string()]);
        assert_eq!(r2.due_date.as_deref(), Some("2026-12-31"));
        // Some(None) clears the due date.
        let r3 = svc
            .update(&id, "登录", None, None, None, Some(None), None, None, "u1")
            .await
            .expect("update");
        assert!(r3.due_date.is_none());
        // Invalid date fails validation.
        assert_eq!(
            svc.update(&id, "登录", None, None, None, Some(Some("2026/12/31")), None, None, "u1")
                .await
                .unwrap_err(),
            RequirementCmdError::Validation(RequirementError::InvalidDueDate("2026/12/31".into()))
        );
    }

    #[tokio::test]
    async fn update_sets_module_none_keeps_and_empty_unfiles() {
        let (svc, id) = seeded().await;
        // Some replaces and logs a module change.
        let r = svc
            .update(&id, "登录", None, None, None, None, None, Some(" mod-1 "), "u1")
            .await
            .expect("update");
        assert_eq!(r.module_id, "mod-1"); // trimmed
                                          // None keeps.
        let r2 = svc
            .update(&id, "登录", None, None, None, None, None, None, "u1")
            .await
            .expect("update");
        assert_eq!(r2.module_id, "mod-1");
        // Same value logs nothing; empty string unfiles the module.
        svc.update(&id, "登录", None, None, None, None, None, Some("mod-1"), "u1")
            .await
            .expect("update");
        let r3 = svc
            .update(&id, "登录", None, None, None, None, None, Some(""), "u1")
            .await
            .expect("update");
        assert_eq!(r3.module_id, "");
        let log = svc.changes(&id).await.expect("changes");
        let modules: Vec<(&str, &str)> = log
            .iter()
            .filter(|c| c.field == "module")
            .map(|c| (c.old_value.as_str(), c.new_value.as_str()))
            .collect();
        // Newest first: unfile (mod-1 → "") then set ("" → mod-1); the same-value update logged nothing.
        assert_eq!(modules, [("mod-1", ""), ("", "mod-1")]);
    }

    #[tokio::test]
    async fn update_replaces_custom_fields_and_logs_per_key_diff() {
        let (svc, id) = seeded().await;
        let cf = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        let r = svc
            .update(
                &id,
                "登录",
                None,
                None,
                None,
                None,
                Some(&cf(&[("owner", "alice"), ("module", "登录")])),
                None,
                "u1",
            )
            .await
            .expect("update");
        assert_eq!(r.custom_fields, cf(&[("owner", "alice"), ("module", "登录")]));
        // None keeps.
        let r2 = svc
            .update(&id, "登录", None, None, None, None, None, None, "u1")
            .await
            .expect("update");
        assert_eq!(r2.custom_fields, cf(&[("owner", "alice"), ("module", "登录")]));
        // Wholesale replace: change owner, drop module, add env.
        let r3 = svc
            .update(
                &id,
                "登录",
                None,
                None,
                None,
                None,
                Some(&cf(&[("owner", "bob"), ("env", "prod")])),
                None,
                "u1",
            )
            .await
            .expect("update");
        assert_eq!(r3.custom_fields, cf(&[("owner", "bob"), ("env", "prod")]));
        // Invalid key fails validation.
        assert_eq!(
            svc.update(&id, "登录", None, None, None, None, Some(&cf(&[("  ", "v")])), None, "u1")
                .await
                .unwrap_err(),
            RequirementCmdError::Validation(RequirementError::EmptyCustomFieldKey)
        );
        // Latest round in the change log: env (added, empty old) / module (removed, empty new) / owner (changed).
        // BTreeMap iterates in key order: existing keys (module/owner) are diffed first, then added keys (env).
        let log = svc.changes(&id).await.expect("changes");
        let head: Vec<(&str, &str, &str)> = log[..3]
            .iter()
            .map(|c| (c.field.as_str(), c.old_value.as_str(), c.new_value.as_str()))
            .collect();
        assert_eq!(
            head,
            [
                ("custom.env", "", "prod"),
                ("custom.owner", "alice", "bob"),
                ("custom.module", "登录", ""),
            ]
        );
        // Resubmitting the same map appends nothing.
        let n = log.len();
        svc.update(&id, "登录", None, None, None, None, Some(&r3.custom_fields), None, "u1")
            .await
            .expect("update");
        assert_eq!(svc.changes(&id).await.expect("changes").len(), n);
    }

    #[tokio::test]
    async fn mutations_record_change_log_newest_first() {
        let (svc, id) = seeded().await;
        svc.update(&id, "登入", Some("P0"), None, None, None, None, None, "alice")
            .await
            .expect("update");
        svc.revise(&id, "v2", &[], "bob").await.expect("revise");
        svc.set_baseline(&id, 2, "carol").await.expect("baseline");
        svc.set_stage(&id, "DEV", Some("IN_PROGRESS"), None, None, "dave").await.expect("stage");
        svc.deliver(&id, "erin").await.expect("deliver");

        let log = svc.changes(&id).await.expect("changes");
        let fields: Vec<&str> = log.iter().map(|c| c.field.as_str()).collect();
        // Newest first: deliver (status + acceptance/delivery hooks) → set_stage(DEV) →
        // baseline (status + review hook) → revise → update (title, priority) → creation hook.
        assert_eq!(
            fields,
            [
                "stage.DELIVERY",
                "stage.ACCEPTANCE",
                "status",
                "stage.DEV",
                "stage.REVIEW",
                "status",
                "version",
                "priority",
                "title",
                "stage.CREATED",
            ]
        );
        assert_eq!(log[0].changed_by, "erin");
        assert_eq!(log[2].old_value, "BASELINED");
        assert_eq!(log[2].new_value, "DELIVERED");
        assert_eq!(log[3].old_value, "PENDING");
        assert_eq!(log[3].new_value, "IN_PROGRESS");
        assert_eq!(log[6].field, "version");
        assert_eq!(log[6].new_value, "2");
        assert_eq!(log[8].old_value, "登录");
        assert_eq!(log[8].new_value, "登入");
        // Timestamps are monotonic (descending).
        assert!(log.windows(2).all(|w| w[0].changed_at_ms >= w[1].changed_at_ms));
    }

    #[tokio::test]
    async fn unchanged_update_and_repeat_status_record_nothing() {
        let (svc, id) = seeded().await;
        // Seed has only the creation-hook entry.
        assert_eq!(svc.changes(&id).await.expect("changes").len(), 1);
        svc.update(&id, "登录", None, None, None, None, None, None, "u1").await.expect("update");
        assert_eq!(svc.changes(&id).await.expect("changes").len(), 1);
        svc.set_stage(&id, "DEV", Some("IN_PROGRESS"), None, None, "u1").await.expect("stage");
        svc.set_stage(&id, "DEV", Some("IN_PROGRESS"), None, None, "u1").await.expect("stage");
        assert_eq!(svc.changes(&id).await.expect("changes").len(), 2);
    }

    #[tokio::test]
    async fn save_bumps_updated_at() {
        let (svc, id) = seeded().await;
        let before = svc.get(&id).await.expect("get").updated_at_ms;
        svc.update(&id, "登入", None, None, None, None, None, None, "u1").await.expect("update");
        let after = svc.get(&id).await.expect("get").updated_at_ms;
        assert!(after > before);
    }
}
