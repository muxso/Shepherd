use std::sync::Arc;

use crate::domain::{ApiScenario, NewApiScenario, NewScenarioStep, ScenarioError};
use crate::ports::{ApiScenarioRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CopyScenarioError {
    #[error("scenario not found")]
    NotFound,
    #[error(transparent)]
    Validation(#[from] ScenarioError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CopyScenarioUseCase {
    repo: Arc<dyn ApiScenarioRepository>,
}

impl CopyScenarioUseCase {
    pub fn new(repo: Arc<dyn ApiScenarioRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        source_id: &str,
        name: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<ApiScenario, CopyScenarioError> {
        let Some(source) = self.repo.get_scenario(source_id).await? else {
            return Err(CopyScenarioError::NotFound);
        };

        let new_name = match name.map(str::trim) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => format!("{}_copy", source.name),
        };

        let created = NewApiScenario::new(&source.project_id, &new_name)?.with_created_by(created_by);
        let fresh = self.repo.insert_scenario(&created).await?;
        self.repo
            .update_scenario(&fresh.id, &new_name, source.status.as_str(), &source.meta)
            .await?;

        for step in &source.steps {
            let cloned = NewScenarioStep::new(
                step.order,
                step.kind.clone(),
                step.ref_mode,
                step.snapshot.clone(),
            )?;
            self.repo.add_step(&fresh.id, &cloned).await?;
        }

        self.repo
            .get_scenario(&fresh.id)
            .await?
            .ok_or(CopyScenarioError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiScenarioRepository;
    use crate::application::{AddStepUseCase, CreateScenarioUseCase};
    use crate::domain::{InlineRequest, RefMode, StepKind};

    async fn seed(repo: Arc<dyn ApiScenarioRepository>) -> String {
        let create = CreateScenarioUseCase::new(repo.clone());
        let s = create.execute("p1", "下单链路", Some("alice")).await.expect("create");
        repo.update_scenario(
            &s.id,
            "下单链路",
            "DEBUGGING",
            &serde_json::json!({ "priority": "P1", "tags": ["核心"] }),
        )
        .await
        .expect("meta");
        let add = AddStepUseCase::new(repo.clone());
        let case = NewScenarioStep::new(0, StepKind::Case { case_id: "c1".into() }, RefMode::Reference, None)
            .expect("case step");
        add.execute(&s.id, &case).await.expect("add case");
        let req = InlineRequest::new("POST", "http://x/order", Some("{}".into())).expect("req");
        let request = NewScenarioStep::new(1, StepKind::Request(req), RefMode::Reference, None)
            .expect("req step");
        add.execute(&s.id, &request).await.expect("add req");
        s.id
    }

    #[tokio::test]
    async fn copies_name_status_meta_and_steps() {
        let repo: Arc<dyn ApiScenarioRepository> = Arc::new(InMemoryApiScenarioRepository::new());
        let src = seed(repo.clone()).await;

        let uc = CopyScenarioUseCase::new(repo.clone());
        let copy = uc.execute(&src, None, Some("bob")).await.expect("copy");

        assert_ne!(copy.id, src);
        assert_eq!(copy.name, "下单链路_copy");
        assert_eq!(copy.project_id, "p1");
        assert_eq!(copy.status.as_str(), "DEBUGGING");
        assert_eq!(copy.meta["priority"], "P1");
        assert_eq!(copy.created_by.as_deref(), Some("bob"));
        assert_eq!(copy.steps.len(), 2);
        assert_eq!(copy.steps[0].kind.kind_str(), "CASE");
        assert_eq!(copy.steps[1].kind.kind_str(), "REQUEST");
        let original = repo.get_scenario(&src).await.expect("get").expect("some");
        assert_eq!(original.steps.len(), 2);
        assert_ne!(copy.steps[0].id, original.steps[0].id);
    }

    #[tokio::test]
    async fn honors_explicit_name() {
        let repo: Arc<dyn ApiScenarioRepository> = Arc::new(InMemoryApiScenarioRepository::new());
        let src = seed(repo.clone()).await;
        let uc = CopyScenarioUseCase::new(repo);
        let copy = uc.execute(&src, Some("  回归_下单  "), None).await.expect("copy");
        assert_eq!(copy.name, "回归_下单");
    }

    #[tokio::test]
    async fn missing_source_is_not_found() {
        let repo: Arc<dyn ApiScenarioRepository> = Arc::new(InMemoryApiScenarioRepository::new());
        let uc = CopyScenarioUseCase::new(repo);
        assert_eq!(uc.execute("ghost", None, None).await.unwrap_err(), CopyScenarioError::NotFound);
    }
}
