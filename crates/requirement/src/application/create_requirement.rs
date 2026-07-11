use std::sync::Arc;

use thiserror::Error;

use crate::domain::{
    parse_priority, parse_req_type, NewRequirement, Requirement, RequirementError,
};
use crate::ports::{RepoError, RequirementRepository};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateRequirementError {
    #[error(transparent)]
    Validation(#[from] RequirementError),
    #[error("requirement title already exists")]
    TitleAlreadyExists,
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
        self.execute_with(project_id, title, description, criteria, None, None).await
    }

    /// 同 `execute`,另接收可选优先级/需求类型原始串:缺省取默认值,非法值报校验错。
    pub async fn execute_with(
        &self,
        project_id: &str,
        title: &str,
        description: &str,
        criteria: &[String],
        priority: Option<&str>,
        req_type: Option<&str>,
    ) -> Result<Requirement, CreateRequirementError> {
        let priority = priority.map(parse_priority).transpose()?.unwrap_or_default();
        let req_type = req_type.map(parse_req_type).transpose()?.unwrap_or_default();
        let new = NewRequirement::new(project_id, title, description, criteria)?
            .with_priority(priority)
            .with_req_type(req_type);

        if self.repo.find_active_by_title(&new.project_id, &new.title).await?.is_some() {
            return Err(CreateRequirementError::TitleAlreadyExists);
        }

        Ok(self.repo.insert(&new).await?)
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
            .execute_with("p1", "登录", "d", &[], Some(" p0 "), Some("tech_debt"))
            .await
            .expect("ok");
        assert_eq!(r.priority, RequirementPriority::P0);
        assert_eq!(r.req_type, RequirementType::TechDebt);
    }

    #[tokio::test]
    async fn rejects_invalid_priority_and_type() {
        assert_eq!(
            uc().execute_with("p1", "登录", "d", &[], Some("P9"), None).await.unwrap_err(),
            CreateRequirementError::Validation(RequirementError::InvalidPriority("P9".into()))
        );
        assert_eq!(
            uc().execute_with("p1", "登录", "d", &[], None, Some("EPIC")).await.unwrap_err(),
            CreateRequirementError::Validation(RequirementError::InvalidReqType("EPIC".into()))
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
