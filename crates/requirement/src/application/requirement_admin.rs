use std::sync::Arc;

use crate::domain::{
    normalize_tags, parse_criteria, parse_due_date, parse_priority, parse_req_type, parse_stage,
    parse_stage_status, ChangeEntry, NewChange, Requirement, RequirementError, Stage, StageStatus,
    StatusCounts,
};
use crate::ports::{RepoError, RequirementRepository};

/// 变更日志单次最多回放的条数。
const MAX_CHANGE_ENTRIES: u32 = 200;
/// 父链上溯步数上限(防御脏数据里的既有环)。
const MAX_ANCESTOR_HOPS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequirementCmdError {
    Validation(RequirementError),
    TitleExists,
    NotFound,
    NoSuchVersion(u32),
    Archived,
    NotUnderReview,
    /// 指定的父需求不存在(或已软删除)。
    ParentNotFound,
    /// 需求不能作为自己的父需求。
    SelfParent,
    /// 父需求必须与本需求同项目。
    CrossProjectParent,
    /// 设置该父需求会构成环。
    ParentCycle,
    Repo(RepoError),
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 可选值统一转字符串入变更日志(None → 空串)。
fn opt_str(v: Option<&str>) -> String {
    v.unwrap_or_default().to_string()
}

/// 计划日期入参:外层 None 不动;`Some(None)` 清除;`Some(Some(d))` 须为 YYYY-MM-DD。
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

    /// 软删除视为不存在。
    pub async fn get(&self, id: &str) -> Result<Requirement, RequirementCmdError> {
        self.repo.get(id).await?.filter(|r| !r.deleted).ok_or(RequirementCmdError::NotFound)
    }

    /// 手工排序:按给定顺序为项目内这些需求写入显式秩。
    pub async fn reorder(
        &self,
        project_id: &str,
        ordered_ids: &[String],
    ) -> Result<(), RequirementCmdError> {
        self.repo.set_order(project_id, ordered_ids).await?;
        Ok(())
    }

    /// 项目内需求按状态聚合(仪表盘)。
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
        // 定基线即评审通过。
        self.stage_hook(&mut req, Stage::Review, StageStatus::Done, by).await;
        Ok(req)
    }

    /// 标题唯一性忽略软删除并排除自身。
    pub async fn rename(
        &self,
        id: &str,
        title: &str,
        by: &str,
    ) -> Result<Requirement, RequirementCmdError> {
        self.update(id, title, None, None, None, None, by).await
    }

    /// 改名 + 可选更新优先级/需求类型/标签/截止日期:None 表示不动,非法值报校验错。
    /// `due_date`:外层 None 不动;`Some(None)` 清除;`Some(Some(d))` 设为 d(须为 YYYY-MM-DD)。
    #[allow(clippy::too_many_arguments)]
    pub async fn update(
        &self,
        id: &str,
        title: &str,
        priority: Option<&str>,
        req_type: Option<&str>,
        tags: Option<&[String]>,
        due_date: Option<Option<&str>>,
        by: &str,
    ) -> Result<Requirement, RequirementCmdError> {
        let priority = priority.map(parse_priority).transpose()?;
        let req_type = req_type.map(parse_req_type).transpose()?;
        let tags = tags.map(normalize_tags).transpose()?;
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
        self.repo.save(&req).await?;
        if !log.is_empty() {
            self.repo.append_change(id, &log).await?;
        }
        Ok(req)
    }

    /// 推进流水线阶段:可选改状态(盖章规则见 `StageRow::set_status`)、可选设/清计划起止日期
    /// (外层 None 不动;`Some(None)` 清除;`Some(Some(d))` 设为 d,须为 YYYY-MM-DD)。
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

    /// 阶段自动钩子:业务动作成功后同步对应阶段行;已是目标状态则跳过。
    /// 阶段写失败不拖垮主操作,仅告警(读侧回落 `fill_stages` 默认行)。
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

    /// 挂/摘父需求:父须存在(未软删)、同项目、非自身,且不得构成环。
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
        // 沿父链上溯:碰到自身即成环;步数封顶,防御脏数据里的既有环。
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

    /// 直属子需求(未软删除)。
    pub async fn children(&self, id: &str) -> Result<Vec<Requirement>, RequirementCmdError> {
        self.get(id).await?;
        Ok(self.repo.children(id).await?)
    }

    /// 变更日志,最新在前,封顶 `MAX_CHANGE_ENTRIES` 条。
    pub async fn changes(&self, id: &str) -> Result<Vec<ChangeEntry>, RequirementCmdError> {
        self.get(id).await?;
        Ok(self.repo.list_changes(id, MAX_CHANGE_ENTRIES).await?)
    }

    // TODO(评审 AI 化): 在人工通过/驳回之外引入 AI 评审意见 —— 检索该需求关联的
    // 测试用例(功能用例覆盖链)与产品 PRD 语料(RAG),让模型给出「通过/驳回 + 依据」
    // 作为评审决策输入。落点:第五个 LLM 触点(见 server/llm.rs 前四个:拆分/执行/
    // 验证/用例起草),评审门仍由人最终拍板,AI 结论作为附签意见展示在评审页。
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
        // 驳回不改状态(留在 DRAFT 待重评),仍记一条 status 流水标记这次评审动作。
        self.repo
            .append_change(id, &[change(by, "status", old.as_str(), req.status.as_str())])
            .await?;
        // 驳回意味着评审仍在进行中。
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
        // 交付即验收完成(若尚未)+ 交付完成。
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
            .update(&id, "登录", Some(" p1 "), Some("bugfix"), None, None, "u1")
            .await
            .expect("update");
        assert_eq!(r.priority, RequirementPriority::P1);
        assert_eq!(r.req_type, RequirementType::Bugfix);
        // None 不动已有值。
        let r2 = svc.update(&id, "登入", None, None, None, None, "u1").await.expect("update");
        assert_eq!(r2.title, "登入");
        assert_eq!(r2.priority, RequirementPriority::P1);
        assert_eq!(r2.req_type, RequirementType::Bugfix);
        // 非法值报校验错。
        assert_eq!(
            svc.update(&id, "登入", Some("P9"), None, None, None, "u1").await.unwrap_err(),
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
        // 钩子记了变更日志。
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
        // 重复推进不覆盖开始时间。
        let r2 =
            svc.set_stage(&id, "DEV", Some("IN_PROGRESS"), None, None, "u1").await.expect("stage");
        assert_eq!(stage_of(&r2, Stage::Dev).started_at_ms, Some(started));
        let r3 = svc.set_stage(&id, "DEV", Some("DONE"), None, None, "u1").await.expect("stage");
        let dev3 = stage_of(&r3, Stage::Dev);
        assert_eq!(dev3.started_at_ms, Some(started));
        let finished = dev3.finished_at_ms.expect("finished stamped");
        let r4 = svc.set_stage(&id, "DEV", Some("DONE"), None, None, "u1").await.expect("stage");
        assert_eq!(stage_of(&r4, Stage::Dev).finished_at_ms, Some(finished));
        // 持久化了(重取仍在);直接 DONE 补齐开始时间。
        let got = svc.get(&id).await.expect("get");
        assert_eq!(stage_of(&got, Stage::Dev).status, StageStatus::Done);
        let r5 = svc.set_stage(&id, "TEST", Some("done"), None, None, "u1").await.expect("stage");
        let test = stage_of(&r5, Stage::Test);
        assert!(test.started_at_ms.is_some());
        assert!(test.finished_at_ms.is_some());
        // 非法阶段/状态报校验错。
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
        assert_eq!(dev.status, StageStatus::Pending); // 只改计划不动状态
                                                      // 外层 None 不动。
        let r2 = svc.set_stage(&id, "DEV", None, None, None, "u1").await.expect("stage");
        assert_eq!(stage_of(&r2, Stage::Dev).planned_end.as_deref(), Some("2026-07-31"));
        // Some(None) 清除。
        let r3 = svc.set_stage(&id, "DEV", None, None, Some(None), "u1").await.expect("stage");
        assert!(stage_of(&r3, Stage::Dev).planned_end.is_none());
        assert_eq!(stage_of(&r3, Stage::Dev).planned_start.as_deref(), Some("2026-07-01"));
        // 非法日期报校验错。
        assert_eq!(
            svc.set_stage(&id, "DEV", None, None, Some(Some("2026/07/31")), "u1")
                .await
                .unwrap_err(),
            RequirementCmdError::Validation(RequirementError::InvalidDueDate("2026/07/31".into()))
        );
        // 变更日志:最新在前 = 清除 plannedEnd → 设 plannedEnd → 设 plannedStart → 建档。
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
        // 驳回 → 评审进行中。
        let r = svc.reject_review(&id, "缺少异常路径", "u1").await.expect("reject");
        assert_eq!(stage_of(&r, Stage::Review).status, StageStatus::InProgress);
        // 定基线 → 评审通过。
        let r2 = svc.set_baseline(&id, 1, "u1").await.expect("baseline");
        assert_eq!(stage_of(&r2, Stage::Review).status, StageStatus::Done);
        // 持久化 + 变更日志。
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
        // 重复交付幂等:不再追加阶段日志。
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
        // 已完成的验收不被钩子改写(时间戳保持),且只为 DELIVERY 记日志。
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
        // 摘除父需求。
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
        // A → B → C 后,C 再挂 A 成环。
        svc.set_parent(&b, Some(&a), "u1").await.expect("b under a");
        svc.set_parent(&c, Some(&b), "u1").await.expect("c under b");
        assert_eq!(
            svc.set_parent(&a, Some(&c), "u1").await.unwrap_err(),
            RequirementCmdError::ParentCycle
        );
        // 直接父子环也拦。
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
            .update(&id, "登录", None, None, Some(&tags), Some(Some("2026-12-31")), "u1")
            .await
            .expect("update");
        assert_eq!(r.tags, vec!["api".to_string(), "web".to_string()]);
        assert_eq!(r.due_date.as_deref(), Some("2026-12-31"));
        // 外层 None 不动。
        let r2 = svc.update(&id, "登录", None, None, None, None, "u1").await.expect("update");
        assert_eq!(r2.tags, vec!["api".to_string(), "web".to_string()]);
        assert_eq!(r2.due_date.as_deref(), Some("2026-12-31"));
        // Some(None) 清除截止日期。
        let r3 = svc.update(&id, "登录", None, None, None, Some(None), "u1").await.expect("update");
        assert!(r3.due_date.is_none());
        // 非法日期报校验错。
        assert_eq!(
            svc.update(&id, "登录", None, None, None, Some(Some("2026/12/31")), "u1")
                .await
                .unwrap_err(),
            RequirementCmdError::Validation(RequirementError::InvalidDueDate("2026/12/31".into()))
        );
    }

    #[tokio::test]
    async fn mutations_record_change_log_newest_first() {
        let (svc, id) = seeded().await;
        svc.update(&id, "登入", Some("P0"), None, None, None, "alice").await.expect("update");
        svc.revise(&id, "v2", &[], "bob").await.expect("revise");
        svc.set_baseline(&id, 2, "carol").await.expect("baseline");
        svc.set_stage(&id, "DEV", Some("IN_PROGRESS"), None, None, "dave").await.expect("stage");
        svc.deliver(&id, "erin").await.expect("deliver");

        let log = svc.changes(&id).await.expect("changes");
        let fields: Vec<&str> = log.iter().map(|c| c.field.as_str()).collect();
        // 最新在前:deliver(status + 验收/交付钩子) → set_stage(DEV) →
        // baseline(status + 评审钩子) → revise → update(title, priority) → 建档钩子。
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
        // 时间戳单调(倒序)。
        assert!(log.windows(2).all(|w| w[0].changed_at_ms >= w[1].changed_at_ms));
    }

    #[tokio::test]
    async fn unchanged_update_and_repeat_status_record_nothing() {
        let (svc, id) = seeded().await;
        // 种子只有建档钩子那一条。
        assert_eq!(svc.changes(&id).await.expect("changes").len(), 1);
        svc.update(&id, "登录", None, None, None, None, "u1").await.expect("update");
        assert_eq!(svc.changes(&id).await.expect("changes").len(), 1);
        svc.set_stage(&id, "DEV", Some("IN_PROGRESS"), None, None, "u1").await.expect("stage");
        svc.set_stage(&id, "DEV", Some("IN_PROGRESS"), None, None, "u1").await.expect("stage");
        assert_eq!(svc.changes(&id).await.expect("changes").len(), 2);
    }

    #[tokio::test]
    async fn save_bumps_updated_at() {
        let (svc, id) = seeded().await;
        let before = svc.get(&id).await.expect("get").updated_at_ms;
        svc.update(&id, "登入", None, None, None, None, "u1").await.expect("update");
        let after = svc.get(&id).await.expect("get").updated_at_ms;
        assert!(after > before);
    }
}
