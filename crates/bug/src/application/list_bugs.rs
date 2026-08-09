use std::sync::Arc;

use crate::domain::Bug;
use crate::ports::{BugRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ListBugsError {
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct ListBugsUseCase {
    repo: Arc<dyn BugRepository>,
}

impl ListBugsUseCase {
    pub fn new(repo: Arc<dyn BugRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, project_id: &str) -> Result<Vec<Bug>, ListBugsError> {
        Ok(self.repo.list(project_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryBugRepository;

    #[tokio::test]
    async fn lists_project_bugs_newest_first() {
        let repo = Arc::new(InMemoryBugRepository::with_default_flow("p1"));
        let create = crate::application::CreateBugUseCase::new(repo.clone());
        create
            .execute(
                "p1",
                "first",
                "NEW",
                None,
                None,
                None,
                None,
                &std::collections::BTreeMap::new(),
            )
            .await
            .expect("ok");
        create
            .execute(
                "p1",
                "second",
                "NEW",
                None,
                None,
                None,
                None,
                &std::collections::BTreeMap::new(),
            )
            .await
            .expect("ok");

        let uc = ListBugsUseCase::new(repo);
        let bugs = uc.execute("p1").await.expect("list");
        assert_eq!(bugs.len(), 2);
        assert_eq!(bugs[0].title, "second");
        assert_eq!(bugs[1].title, "first");
    }

    #[tokio::test]
    async fn isolates_by_project() {
        let repo = Arc::new(InMemoryBugRepository::with_default_flow("p1"));
        crate::application::CreateBugUseCase::new(repo.clone())
            .execute(
                "p1",
                "boom",
                "NEW",
                None,
                None,
                None,
                None,
                &std::collections::BTreeMap::new(),
            )
            .await
            .expect("ok");

        let uc = ListBugsUseCase::new(repo);
        assert_eq!(uc.execute("p1").await.expect("list").len(), 1);
        assert!(uc.execute("p2").await.expect("list").is_empty());
    }
}
