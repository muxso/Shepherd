use std::sync::Arc;

use crate::domain::StatusFlowGraph;
use crate::ports::{BugRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BugStatusFlowError {
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct BugStatusFlowUseCase {
    repo: Arc<dyn BugRepository>,
}

impl BugStatusFlowUseCase {
    pub fn new(repo: Arc<dyn BugRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, project_id: &str) -> Result<StatusFlowGraph, BugStatusFlowError> {
        Ok(self.repo.status_flow(project_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryBugRepository;

    #[tokio::test]
    async fn returns_project_flow() {
        let repo = Arc::new(InMemoryBugRepository::with_default_flow("p1"));
        let uc = BugStatusFlowUseCase::new(repo);
        let flow = uc.execute("p1").await.expect("flow");
        assert!(flow.contains("NEW"));
        assert!(flow.can_transition("NEW", "RESOLVED"));
        assert_eq!(flow.targets("RESOLVED"), vec!["CLOSED", "REOPENED"]);
    }
}
