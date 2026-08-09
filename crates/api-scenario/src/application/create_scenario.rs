use std::sync::Arc;

use crate::domain::{ApiScenario, NewApiScenario, ScenarioError};
use crate::ports::{ApiScenarioRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateScenarioError {
    #[error(transparent)]
    Validation(#[from] ScenarioError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateScenarioUseCase {
    repo: Arc<dyn ApiScenarioRepository>,
}

impl CreateScenarioUseCase {
    pub fn new(repo: Arc<dyn ApiScenarioRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        project_id: &str,
        name: &str,
        created_by: Option<&str>,
    ) -> Result<ApiScenario, CreateScenarioError> {
        let new_scenario = NewApiScenario::new(project_id, name)?.with_created_by(created_by);
        Ok(self.repo.insert_scenario(&new_scenario).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiScenarioRepository;

    #[tokio::test]
    async fn creates_scenario_with_empty_steps() {
        let repo = InMemoryApiScenarioRepository::new();
        let uc = CreateScenarioUseCase::new(Arc::new(repo));
        let s = uc.execute("p1", "order placement flow", None).await.expect("ok");
        assert_eq!(s.name, "order placement flow");
        assert_eq!(s.project_id, "p1");
        assert!(s.steps.is_empty());
    }

    #[tokio::test]
    async fn rejects_blank_name() {
        let repo = InMemoryApiScenarioRepository::new();
        let uc = CreateScenarioUseCase::new(Arc::new(repo));
        let err = uc.execute("p1", "  ", None).await.unwrap_err();
        assert_eq!(err, CreateScenarioError::Validation(ScenarioError::EmptyName));
    }
}
