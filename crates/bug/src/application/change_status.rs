use std::sync::Arc;

use crate::domain::{Bug, BugError};
use crate::ports::{BugRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ChangeBugStatusError {
    #[error("bug not found")]
    BugNotFound,
    #[error(transparent)]
    Domain(#[from] BugError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct ChangeBugStatusUseCase {
    repo: Arc<dyn BugRepository>,
}

impl ChangeBugStatusUseCase {
    pub fn new(repo: Arc<dyn BugRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, bug_id: &str, to: &str) -> Result<Bug, ChangeBugStatusError> {
        let mut bug = self.repo.get(bug_id).await?.ok_or(ChangeBugStatusError::BugNotFound)?;
        let flow = self.repo.status_flow(&bug.project_id).await?;

        bug.change_status(to, &flow)?;

        self.repo.set_status(&bug.id, &bug.status).await?;
        Ok(bug)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryBugRepository;
    use crate::application::CreateBugUseCase;

    async fn seed_bug(repo: &InMemoryBugRepository) -> String {
        let create = CreateBugUseCase::new(Arc::new(repo.clone()));
        create.execute("p1", "boom", "NEW", None).await.expect("seed").id
    }

    #[tokio::test]
    async fn valid_transition_persists_new_status() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let id = seed_bug(&repo).await;
        let uc = ChangeBugStatusUseCase::new(Arc::new(repo.clone()));

        let bug = uc.execute(&id, "RESOLVED").await.expect("ok");
        assert_eq!(bug.status, "RESOLVED");
        assert_eq!(repo.status_of(&id), Some("RESOLVED".to_string()));
    }

    #[tokio::test]
    async fn disallowed_transition_rejected_and_not_persisted() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let id = seed_bug(&repo).await;
        let uc = ChangeBugStatusUseCase::new(Arc::new(repo.clone()));

        let err = uc.execute(&id, "CLOSED").await.unwrap_err();
        assert_eq!(
            err,
            ChangeBugStatusError::Domain(BugError::TransitionNotAllowed {
                from: "NEW".into(),
                to: "CLOSED".into()
            })
        );
        assert_eq!(repo.status_of(&id), Some("NEW".to_string()));
    }

    #[tokio::test]
    async fn missing_bug_is_not_found() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = ChangeBugStatusUseCase::new(Arc::new(repo));
        assert_eq!(uc.execute("ghost", "RESOLVED").await, Err(ChangeBugStatusError::BugNotFound));
    }

    #[tokio::test]
    async fn full_lifecycle_new_resolved_closed_reopened() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let id = seed_bug(&repo).await;
        let uc = ChangeBugStatusUseCase::new(Arc::new(repo.clone()));

        uc.execute(&id, "RESOLVED").await.expect("new->resolved");
        uc.execute(&id, "CLOSED").await.expect("resolved->closed");
        uc.execute(&id, "REOPENED").await.expect("closed->reopened");
        assert_eq!(repo.status_of(&id), Some("REOPENED".to_string()));
    }
}
