use std::collections::BTreeMap;
use std::sync::Arc;

use crate::domain::{normalize_custom_fields, Bug, BugError, NewBug};
use crate::ports::{BugRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateBugError {
    #[error(transparent)]
    Validation(#[from] BugError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateBugUseCase {
    repo: Arc<dyn BugRepository>,
}

impl CreateBugUseCase {
    pub fn new(repo: Arc<dyn BugRepository>) -> Self {
        Self { repo }
    }

    /// 组装用:暴露底层仓储,供构造共享同一存储的其他用例(如自定义字段用例)。
    pub fn repo(&self) -> Arc<dyn BugRepository> {
        self.repo.clone()
    }

    pub async fn execute(
        &self,
        project_id: &str,
        title: &str,
        initial_status: &str,
        created_by: Option<&str>,
        custom_fields: &BTreeMap<String, String>,
    ) -> Result<Bug, CreateBugError> {
        let custom_fields = normalize_custom_fields(custom_fields)?;
        let new_bug = NewBug::new(project_id, title)?
            .with_created_by(created_by)
            .with_custom_fields(custom_fields);

        let flow = self.repo.status_flow(project_id).await?;
        if !flow.contains(initial_status) {
            return Err(CreateBugError::Validation(BugError::UnknownStatus(
                initial_status.to_string(),
            )));
        }

        Ok(self.repo.insert(&new_bug, initial_status).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryBugRepository;

    #[tokio::test]
    async fn creates_bug_with_valid_initial_status() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = CreateBugUseCase::new(Arc::new(repo));
        let bug =
            uc.execute("p1", "登录崩溃", "NEW", Some("alice"), &BTreeMap::new()).await.expect("ok");
        assert_eq!(bug.status, "NEW");
        assert_eq!(bug.title, "登录崩溃");
        assert_eq!(bug.created_by.as_deref(), Some("alice"));
        assert!(bug.custom_fields.is_empty());
    }

    #[tokio::test]
    async fn creates_bug_with_custom_fields_normalized() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = CreateBugUseCase::new(Arc::new(repo));
        let raw = BTreeMap::from([(" severity ".to_string(), "P0".to_string())]);
        let bug = uc.execute("p1", "登录崩溃", "NEW", None, &raw).await.expect("ok");
        assert_eq!(bug.custom_fields, BTreeMap::from([("severity".to_string(), "P0".to_string())]));
        // 空白键报校验错。
        let bad = BTreeMap::from([("  ".to_string(), "v".to_string())]);
        assert_eq!(
            uc.execute("p1", "又崩了", "NEW", None, &bad).await.unwrap_err(),
            CreateBugError::Validation(BugError::EmptyCustomFieldKey)
        );
    }

    #[tokio::test]
    async fn rejects_unknown_initial_status() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = CreateBugUseCase::new(Arc::new(repo));
        let err = uc.execute("p1", "x", "GHOST", None, &BTreeMap::new()).await.unwrap_err();
        assert_eq!(err, CreateBugError::Validation(BugError::UnknownStatus("GHOST".into())));
    }

    #[tokio::test]
    async fn rejects_blank_title() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = CreateBugUseCase::new(Arc::new(repo));
        let err = uc.execute("p1", "  ", "NEW", None, &BTreeMap::new()).await.unwrap_err();
        assert_eq!(err, CreateBugError::Validation(BugError::EmptyTitle));
    }
}
