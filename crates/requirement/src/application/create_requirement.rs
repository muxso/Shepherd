//! 用例:创建需求。
//!
//! 编排规则:同项目内标题唯一,但**忽略已软删除**的需求(端口的 `find_active_by_title`
//! 已固化此语义)。创建即生成版本 1、baseline=1、状态 Draft。

use std::sync::Arc;

use thiserror::Error;

use crate::domain::{NewRequirement, Requirement, RequirementError};
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
        let new = NewRequirement::new(project_id, title, description, criteria)?;

        if self
            .repo
            .find_active_by_title(&new.project_id, &new.title)
            .await?
            .is_some()
        {
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
    async fn propagates_validation_error() {
        assert_eq!(
            uc().execute("p1", "  ", "d", &[]).await.unwrap_err(),
            CreateRequirementError::Validation(RequirementError::EmptyTitle)
        );
    }
}
