use std::collections::BTreeMap;
use std::sync::Arc;

use thiserror::Error;

use crate::domain::{
    normalize_custom_fields, normalize_tags, parse_due_date, parse_priority, parse_req_type,
    NewChange, NewRequirement, Requirement, RequirementError, Stage, StageRow, StageStatus,
};
use crate::ports::{RepoError, RequirementRepository};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateRequirementError {
    #[error(transparent)]
    Validation(#[from] RequirementError),
    #[error("requirement title already exists")]
    TitleAlreadyExists,
    #[error("parent requirement not found")]
    ParentNotFound,
    #[error("parent requirement belongs to another project")]
    CrossProjectParent,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateRequirementUseCase {
    repo: Arc<dyn RequirementRepository>,
}

impl CreateRequirementUseCase {
    pub fn new(repo: Arc<dyn RequirementRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        project_id: &str,
        title: &str,
        description: &str,
        criteria: &[String],
    ) -> Result<Requirement, CreateRequirementError> {
        self.execute_with(
            project_id,
            title,
            description,
            criteria,
            None,
            None,
            &[],
            None,
            None,
            &BTreeMap::new(),
            "",
            &[],
            "",
        )
        .await
    }

    /// Like `execute`, but also takes optional priority/type/tags/due date/
    /// parent/custom fields/module/skill-ids/creator: missing values fall back to
    /// defaults, invalid ones are validation errors; the parent must exist
    /// (not soft-deleted) and belong to the same project; empty `module_id` =
    /// unfiled (modules live in the shared ms_module table, no existence check).
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_with(
        &self,
        project_id: &str,
        title: &str,
        description: &str,
        criteria: &[String],
        priority: Option<&str>,
        req_type: Option<&str>,
        tags: &[String],
        due_date: Option<&str>,
        parent_id: Option<&str>,
        custom_fields: &BTreeMap<String, String>,
        module_id: &str,
        skill_ids: &[String],
        by: &str,
    ) -> Result<Requirement, CreateRequirementError> {
        let priority = priority.map(parse_priority).transpose()?.unwrap_or_default();
        let req_type = req_type.map(parse_req_type).transpose()?.unwrap_or_default();
        let tags = normalize_tags(tags)?;
        let custom_fields = normalize_custom_fields(custom_fields)?;
        // Empty string means unset, not an invalid date.
        let due_date =
            due_date.map(str::trim).filter(|d| !d.is_empty()).map(parse_due_date).transpose()?;
        let mut new = NewRequirement::new(project_id, title, description, criteria)?
            .with_priority(priority)
            .with_req_type(req_type)
            .with_tags(tags)
            .with_due_date(due_date)
            .with_custom_fields(custom_fields)
            .with_module(module_id)
            .with_skill_ids(skill_ids.to_vec())
            .with_created_by(by);

        if let Some(pid) = parent_id.map(str::trim).filter(|p| !p.is_empty()) {
            let parent = self
                .repo
                .get(pid)
                .await?
                .filter(|p| !p.deleted)
                .ok_or(CreateRequirementError::ParentNotFound)?;
            if parent.project_id != new.project_id {
                return Err(CreateRequirementError::CrossProjectParent);
            }
            new = new.with_parent(Some(pid.to_string()));
        }

        if self.repo.find_active_by_title(&new.project_id, &new.title).await?.is_some() {
            return Err(CreateRequirementError::TitleAlreadyExists);
        }

        let mut req = self.repo.insert(&new).await?;
        self.stamp_created_stage(&mut req, &new.created_by).await;
        Ok(req)
    }

    /// Creation completes the CREATED stage right away (start = finish =
    /// created_at) and logs the change; a write failure does not fail the
    /// creation, it only warns (read side falls back to the PENDING default row).
    async fn stamp_created_stage(&self, req: &mut Requirement, by: &str) {
        let mut row = StageRow::pending(Stage::Created);
        row.set_status(StageStatus::Done, req.created_at_ms);
        if let Err(e) = self.repo.upsert_stage(&req.id, &row).await {
            tracing::warn!(requirement = %req.id, error = %e, "CREATED 阶段写入失败");
            return;
        }
        if let Some(slot) = req.stage_row_mut(Stage::Created) {
            *slot = row;
        }
        let entry = NewChange {
            changed_by: by.to_string(),
            field: "stage.CREATED".to_string(),
            old_value: StageStatus::Pending.as_str().to_string(),
            new_value: StageStatus::Done.as_str().to_string(),
        };
        if let Err(e) = self.repo.append_change(&req.id, &[entry]).await {
            tracing::warn!(requirement = %req.id, error = %e, "CREATED 阶段变更日志写入失败");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryRequirementRepository;

    fn uc() -> CreateRequirementUseCase {
        CreateRequirementUseCase::new(Arc::new(InMemoryRequirementRepository::new()))
    }

    #[tokio::test]
    async fn creates_with_version_1_and_baseline_1() {
        let r = uc().execute("p1", "登录", "desc", &["c1".to_string()]).await.expect("ok");
        assert_eq!(r.latest_version(), 1);
        assert_eq!(r.baseline_version, 1);
        assert_eq!(r.baseline().acceptance_criteria.len(), 1);
    }

    #[tokio::test]
    async fn rejects_duplicate_title_in_same_project() {
        let uc = uc();
        uc.execute("p1", "登录", "d", &[]).await.expect("first");
        assert_eq!(
            uc.execute("p1", "登录", "d", &[]).await.unwrap_err(),
            CreateRequirementError::TitleAlreadyExists
        );
    }

    #[tokio::test]
    async fn allows_same_title_in_different_project() {
        let uc = uc();
        uc.execute("p1", "登录", "d", &[]).await.expect("ok");
        assert!(uc.execute("p2", "登录", "d", &[]).await.is_ok());
    }

    #[tokio::test]
    async fn recreatable_after_soft_delete() {
        let repo = Arc::new(InMemoryRequirementRepository::new());
        let uc = CreateRequirementUseCase::new(repo.clone());
        let r = uc.execute("p1", "登录", "d", &[]).await.expect("ok");
        repo.soft_delete(&r.id);
        assert!(uc.execute("p1", "登录", "d", &[]).await.is_ok());
    }

    #[tokio::test]
    async fn creates_with_default_priority_and_type() {
        use crate::domain::{RequirementPriority, RequirementType};
        let r = uc().execute("p1", "登录", "d", &[]).await.expect("ok");
        assert_eq!(r.priority, RequirementPriority::P2);
        assert_eq!(r.req_type, RequirementType::Feature);
    }

    #[tokio::test]
    async fn creates_with_explicit_priority_and_type_normalized() {
        use crate::domain::{RequirementPriority, RequirementType};
        let r = uc()
            .execute_with(
                "p1",
                "登录",
                "d",
                &[],
                Some(" p0 "),
                Some("tech_debt"),
                &[],
                None,
                None,
                &BTreeMap::new(),
                "",
                &[],
                "",
            )
            .await
            .expect("ok");
        assert_eq!(r.priority, RequirementPriority::P0);
        assert_eq!(r.req_type, RequirementType::TechDebt);
    }

    #[tokio::test]
    async fn rejects_invalid_priority_and_type() {
        assert_eq!(
            uc().execute_with(
                "p1",
                "登录",
                "d",
                &[],
                Some("P9"),
                None,
                &[],
                None,
                None,
                &BTreeMap::new(),
                "",
                &[],
                ""
            )
            .await
            .unwrap_err(),
            CreateRequirementError::Validation(RequirementError::InvalidPriority("P9".into()))
        );
        assert_eq!(
            uc().execute_with(
                "p1",
                "登录",
                "d",
                &[],
                None,
                Some("EPIC"),
                &[],
                None,
                None,
                &BTreeMap::new(),
                "",
                &[],
                ""
            )
            .await
            .unwrap_err(),
            CreateRequirementError::Validation(RequirementError::InvalidReqType("EPIC".into()))
        );
    }

    #[tokio::test]
    async fn creates_with_tags_due_date_and_parent() {
        let repo = Arc::new(InMemoryRequirementRepository::new());
        let uc = CreateRequirementUseCase::new(repo.clone());
        let parent = uc.execute("p1", "父需求", "d", &[]).await.expect("parent");
        let r = uc
            .execute_with(
                "p1",
                "子需求",
                "d",
                &[],
                None,
                None,
                &[" api ".to_string(), "api".to_string(), "web".to_string()],
                Some("2026-12-31"),
                Some(&parent.id),
                &BTreeMap::new(),
                "",
                &[],
                "",
            )
            .await
            .expect("ok");
        assert_eq!(r.tags, vec!["api".to_string(), "web".to_string()]);
        assert_eq!(r.due_date.as_deref(), Some("2026-12-31"));
        assert_eq!(r.parent_id.as_deref(), Some(parent.id.as_str()));
        // An empty-string date means unset.
        let r2 = uc
            .execute_with(
                "p1",
                "另一条",
                "d",
                &[],
                None,
                None,
                &[],
                Some("  "),
                None,
                &BTreeMap::new(),
                "",
                &[],
                "",
            )
            .await
            .expect("ok");
        assert!(r2.due_date.is_none());
    }

    #[tokio::test]
    async fn creates_with_module_and_defaults_unfiled() {
        let uc = uc();
        let r = uc
            .execute_with(
                "p1",
                "登录",
                "d",
                &[],
                None,
                None,
                &[],
                None,
                None,
                &BTreeMap::new(),
                " mod-1 ",
                &[],
                "u1",
            )
            .await
            .expect("ok");
        assert_eq!(r.module_id, "mod-1"); // trimmed
        let r2 = uc.execute("p1", "注册", "d", &[]).await.expect("ok");
        assert_eq!(r2.module_id, ""); // defaults to unfiled
    }

    #[tokio::test]
    async fn creates_with_skill_ids() {
        let uc = uc();
        let r = uc
            .execute_with(
                "p1",
                "登录",
                "d",
                &[],
                None,
                None,
                &[],
                None,
                None,
                &BTreeMap::new(),
                "",
                &["sk-1".to_string(), "sk-2".to_string()],
                "",
            )
            .await
            .expect("ok");
        assert_eq!(r.skill_ids, vec!["sk-1".to_string(), "sk-2".to_string()]);
        // Empty selection defaults to none.
        let r2 = uc
            .execute_with(
                "p1",
                "注册",
                "d",
                &[],
                None,
                None,
                &[],
                None,
                None,
                &BTreeMap::new(),
                "",
                &[],
                "",
            )
            .await
            .expect("ok");
        assert!(r2.skill_ids.is_empty());
    }

    #[tokio::test]
    async fn creates_with_custom_fields_normalized_and_rejects_invalid() {
        let raw = BTreeMap::from([(" owner ".to_string(), "alice".to_string())]);
        let r = uc()
            .execute_with("p1", "登录", "d", &[], None, None, &[], None, None, &raw, "", &[], "")
            .await
            .expect("ok");
        assert_eq!(r.custom_fields, BTreeMap::from([("owner".to_string(), "alice".to_string())]));
        // A blank key is a validation error.
        let bad = BTreeMap::from([("  ".to_string(), "v".to_string())]);
        assert_eq!(
            uc().execute_with(
                "p1",
                "注册",
                "d",
                &[],
                None,
                None,
                &[],
                None,
                None,
                &bad,
                "",
                &[],
                ""
            )
            .await
            .unwrap_err(),
            CreateRequirementError::Validation(RequirementError::EmptyCustomFieldKey)
        );
    }

    #[tokio::test]
    async fn rejects_missing_or_cross_project_parent() {
        let repo = Arc::new(InMemoryRequirementRepository::new());
        let uc = CreateRequirementUseCase::new(repo.clone());
        let other = uc.execute("p2", "别家", "d", &[]).await.expect("other");
        assert_eq!(
            uc.execute_with(
                "p1",
                "子",
                "d",
                &[],
                None,
                None,
                &[],
                None,
                Some("ghost"),
                &BTreeMap::new(),
                "",
                &[],
                ""
            )
            .await
            .unwrap_err(),
            CreateRequirementError::ParentNotFound
        );
        assert_eq!(
            uc.execute_with(
                "p1",
                "子",
                "d",
                &[],
                None,
                None,
                &[],
                None,
                Some(&other.id),
                &BTreeMap::new(),
                "",
                &[],
                ""
            )
            .await
            .unwrap_err(),
            CreateRequirementError::CrossProjectParent
        );
        // A soft-deleted parent counts as missing.
        let doomed = uc.execute("p1", "亡父", "d", &[]).await.expect("p");
        repo.soft_delete(&doomed.id);
        assert_eq!(
            uc.execute_with(
                "p1",
                "子",
                "d",
                &[],
                None,
                None,
                &[],
                None,
                Some(&doomed.id),
                &BTreeMap::new(),
                "",
                &[],
                ""
            )
            .await
            .unwrap_err(),
            CreateRequirementError::ParentNotFound
        );
    }

    #[tokio::test]
    async fn rejects_invalid_due_date_and_tags() {
        assert_eq!(
            uc().execute_with(
                "p1",
                "登录",
                "d",
                &[],
                None,
                None,
                &[],
                Some("2026-13-01"),
                None,
                &BTreeMap::new(),
                "",
                &[],
                ""
            )
            .await
            .unwrap_err(),
            CreateRequirementError::Validation(RequirementError::InvalidDueDate(
                "2026-13-01".into()
            ))
        );
        let many: Vec<String> = (0..11).map(|i| format!("t{i}")).collect();
        assert_eq!(
            uc().execute_with(
                "p1",
                "登录",
                "d",
                &[],
                None,
                None,
                &many,
                None,
                None,
                &BTreeMap::new(),
                "",
                &[],
                ""
            )
            .await
            .unwrap_err(),
            CreateRequirementError::Validation(RequirementError::TooManyTags)
        );
    }

    #[tokio::test]
    async fn propagates_validation_error() {
        assert_eq!(
            uc().execute("p1", "  ", "d", &[]).await.unwrap_err(),
            CreateRequirementError::Validation(RequirementError::EmptyTitle)
        );
    }
}
