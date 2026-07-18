use std::sync::Arc;

use crate::domain::{NewScenarioStep, ScenarioStep};
use crate::ports::{ApiScenarioRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UpdateStepError {
    #[error("scenario or step not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct UpdateStepUseCase {
    repo: Arc<dyn ApiScenarioRepository>,
}

impl UpdateStepUseCase {
    pub fn new(repo: Arc<dyn ApiScenarioRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        scenario_id: &str,
        step_id: &str,
        step: &NewScenarioStep,
    ) -> Result<ScenarioStep, UpdateStepError> {
        if self.repo.get_scenario(scenario_id).await?.is_none() {
            return Err(UpdateStepError::NotFound);
        }
        match self.repo.update_step(scenario_id, step_id, step).await? {
            Some(s) => Ok(s),
            None => Err(UpdateStepError::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiScenarioRepository;
    use crate::application::{AddStepUseCase, CreateScenarioUseCase};
    use crate::domain::{InlineRequest, RefMode, StepKind};

    fn request_step(assertions: serde_json::Value) -> NewScenarioStep {
        let req =
            InlineRequest::new("GET", "http://x", None).expect("valid").with_assertions(assertions);
        NewScenarioStep::new(0, StepKind::Request(Box::new(req)), RefMode::Reference, None)
            .expect("valid")
    }

    #[tokio::test]
    async fn replaces_step_payload_keeping_id_and_order() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let s = CreateScenarioUseCase::new(repo.clone()).execute("p1", "a", None).await.expect("a");
        let added = AddStepUseCase::new(repo.clone())
            .execute(&s.id, &request_step(serde_json::json!([])))
            .await
            .expect("added");

        let asserts = serde_json::json!([{"type": "StatusIs", "args": 201}]);
        let updated = UpdateStepUseCase::new(repo)
            .execute(&s.id, &added.id, &request_step(asserts.clone()))
            .await
            .expect("updated");
        assert_eq!(updated.id, added.id);
        assert_eq!(updated.order, added.order);
        match updated.kind {
            StepKind::Request(r) => assert_eq!(r.assertions, asserts),
            other => panic!("expected request step, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_scenario_or_step_is_not_found() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let uc = UpdateStepUseCase::new(repo.clone());
        let step = request_step(serde_json::json!([]));
        assert_eq!(uc.execute("ghost", "s1", &step).await, Err(UpdateStepError::NotFound));

        let s = CreateScenarioUseCase::new(repo).execute("p1", "a", None).await.expect("a");
        assert_eq!(uc.execute(&s.id, "ghost-step", &step).await, Err(UpdateStepError::NotFound));
    }
}
