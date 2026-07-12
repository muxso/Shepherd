use std::sync::Arc;

use crate::domain::{NewScenarioStep, ScenarioStep};
use crate::ports::{ApiScenarioRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AddStepError {
    #[error("scenario not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct AddStepUseCase {
    repo: Arc<dyn ApiScenarioRepository>,
}

impl AddStepUseCase {
    pub fn new(repo: Arc<dyn ApiScenarioRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        scenario_id: &str,
        step: &NewScenarioStep,
    ) -> Result<ScenarioStep, AddStepError> {
        if self.repo.get_scenario(scenario_id).await?.is_none() {
            return Err(AddStepError::NotFound);
        }
        Ok(self.repo.add_step(scenario_id, step).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiScenarioRepository;
    use crate::application::CreateScenarioUseCase;
    use crate::domain::{InlineRequest, RefMode, StepKind};

    #[tokio::test]
    async fn adds_step_to_existing_scenario() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let create = CreateScenarioUseCase::new(repo.clone());
        let s = create.execute("p1", "a", None).await.expect("a");

        let uc = AddStepUseCase::new(repo);
        let req = InlineRequest::new("GET", "http://x", None).expect("valid");
        let step = NewScenarioStep::new(0, StepKind::Request(req), RefMode::Reference, None)
            .expect("valid");
        let stored = uc.execute(&s.id, &step).await.expect("added");
        assert_eq!(stored.order, 0);
        assert_eq!(stored.kind.kind_str(), "REQUEST");
    }

    #[tokio::test]
    async fn add_step_to_missing_scenario_is_not_found() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let uc = AddStepUseCase::new(repo);
        let step = NewScenarioStep::new(
            0,
            StepKind::Case { case_id: "c1".into() },
            RefMode::Reference,
            None,
        )
        .expect("valid");
        assert_eq!(uc.execute("ghost", &step).await, Err(AddStepError::NotFound));
    }
}
