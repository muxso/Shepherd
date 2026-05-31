//! 用例:创建缺陷。初始状态必须是该项目状态流图里的合法状态。

use std::sync::Arc;

use crate::domain::{Bug, BugError, NewBug};
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

    pub async fn execute(
        &self,
        project_id: &str,
        title: &str,
        initial_status: &str,
    ) -> Result<Bug, CreateBugError> {
        let new_bug = NewBug::new(project_id, title)?;

        // 初始状态必须存在于项目状态流图(不能用一个图里没有的状态建缺陷)
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
        let bug = uc.execute("p1", "登录崩溃", "NEW").await.expect("ok");
        assert_eq!(bug.status, "NEW");
        assert_eq!(bug.title, "登录崩溃");
    }

    #[tokio::test]
    async fn rejects_unknown_initial_status() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = CreateBugUseCase::new(Arc::new(repo));
        let err = uc.execute("p1", "x", "GHOST").await.unwrap_err();
        assert_eq!(err, CreateBugError::Validation(BugError::UnknownStatus("GHOST".into())));
    }

    #[tokio::test]
    async fn rejects_blank_title() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = CreateBugUseCase::new(Arc::new(repo));
        let err = uc.execute("p1", "  ", "NEW").await.unwrap_err();
        assert_eq!(err, CreateBugError::Validation(BugError::EmptyTitle));
    }
}
