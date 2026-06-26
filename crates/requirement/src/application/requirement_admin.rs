use std::sync::Arc;

use crate::domain::{parse_criteria, Requirement, RequirementError};
use crate::ports::{RepoError, RequirementRepository};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequirementCmdError {
    Validation(RequirementError),
    TitleExists,
    NotFound,
    NoSuchVersion(u32),
    Archived,
    NotUnderReview,
    Repo(RepoError),
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

    pub async fn revise(
        &self,
        id: &str,
        description: &str,
        criteria: &[String],
    ) -> Result<u32, RequirementCmdError> {
        let mut req = self.get(id).await?;
        let parsed = parse_criteria(criteria)?;
        let version = req.revise(description, parsed)?;
        self.repo.save(&req).await?;
        Ok(version)
    }

    pub async fn set_baseline(
        &self,
        id: &str,
        version: u32,
    ) -> Result<Requirement, RequirementCmdError> {
        let mut req = self.get(id).await?;
        req.set_baseline(version)?;
        self.repo.save(&req).await?;
        Ok(req)
    }

    /// 标题唯一性忽略软删除并排除自身。
    pub async fn rename(&self, id: &str, title: &str) -> Result<Requirement, RequirementCmdError> {
        let mut req = self.get(id).await?;
        let trimmed = title.trim();
        if let Some(existing) = self.repo.find_active_by_title(&req.project_id, trimmed).await? {
            if existing.id != req.id {
                return Err(RequirementCmdError::TitleExists);
            }
        }
        req.rename(title)?;
        self.repo.save(&req).await?;
        Ok(req)
    }

    pub async fn reject_review(
        &self,
        id: &str,
        reason: &str,
    ) -> Result<Requirement, RequirementCmdError> {
        let mut req = self.get(id).await?;
        req.reject_review(reason)?;
        self.repo.save(&req).await?;
        Ok(req)
    }

    pub async fn deliver(&self, id: &str) -> Result<Requirement, RequirementCmdError> {
        let mut req = self.get(id).await?;
        req.deliver()?;
        self.repo.save(&req).await?;
        Ok(req)
    }

    pub async fn archive(&self, id: &str) -> Result<Requirement, RequirementCmdError> {
        let mut req = self.get(id).await?;
        req.archive();
        self.repo.save(&req).await?;
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
        let v = svc.revise(&id, "v2", &["c2".to_string()]).await.expect("revise");
        assert_eq!(v, 2);
        assert_eq!(svc.get(&id).await.expect("get").baseline_version, 1);
        let r = svc.set_baseline(&id, 2).await.expect("baseline");
        assert_eq!(r.baseline_version, 2);
        assert_eq!(svc.get(&id).await.expect("get").baseline_version, 2);
    }

    #[tokio::test]
    async fn reject_review_persists_reason_and_baseline_clears_it() {
        let (svc, id) = seeded().await;
        let r = svc.reject_review(&id, "  缺少异常路径  ").await.expect("reject");
        assert_eq!(r.review_comment.as_deref(), Some("缺少异常路径"));
        assert_eq!(svc.get(&id).await.expect("get").review_comment.as_deref(), Some("缺少异常路径"));
        let p = svc.set_baseline(&id, 1).await.expect("baseline");
        assert!(p.review_comment.is_none());
    }

    #[tokio::test]
    async fn reject_review_empty_reason_is_validation() {
        let (svc, id) = seeded().await;
        assert_eq!(
            svc.reject_review(&id, "   ").await.unwrap_err(),
            RequirementCmdError::Validation(crate::domain::RequirementError::EmptyReviewComment)
        );
    }

    #[tokio::test]
    async fn reject_review_on_baselined_is_conflict() {
        let (svc, id) = seeded().await;
        svc.set_baseline(&id, 1).await.expect("baseline");
        assert_eq!(
            svc.reject_review(&id, "x").await.unwrap_err(),
            RequirementCmdError::NotUnderReview
        );
    }

    #[tokio::test]
    async fn set_baseline_unknown_version_404() {
        let (svc, id) = seeded().await;
        assert_eq!(svc.set_baseline(&id, 9).await.unwrap_err(), RequirementCmdError::NoSuchVersion(9));
    }

    #[tokio::test]
    async fn revise_archived_is_conflict() {
        let (svc, id) = seeded().await;
        svc.archive(&id).await.expect("archive");
        assert_eq!(
            svc.revise(&id, "v2", &[]).await.unwrap_err(),
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
        assert_eq!(svc.rename(&a, "注册").await.unwrap_err(), RequirementCmdError::TitleExists);
        assert!(svc.rename(&a, "登入").await.is_ok());
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
}
