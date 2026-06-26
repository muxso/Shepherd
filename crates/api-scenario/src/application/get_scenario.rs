use std::sync::Arc;

use crate::domain::ApiScenario;
use crate::ports::{ApiScenarioRepository, RepoError};

#[derive(Clone)]
pub struct GetScenarioUseCase {
    repo: Arc<dyn ApiScenarioRepository>,
}

impl GetScenarioUseCase {
    pub fn new(repo: Arc<dyn ApiScenarioRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, id: &str) -> Result<Option<ApiScenario>, RepoError> {
        self.repo.get_scenario(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiScenarioRepository;
    use crate::application::CreateScenarioUseCase;

    #[tokio::test]
    async fn gets_existing_scenario_and_none_for_missing() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let create = CreateScenarioUseCase::new(repo.clone());
        let s = create.execute("p1", "a", None).await.expect("a");

        let uc = GetScenarioUseCase::new(repo);
        assert_eq!(uc.execute(&s.id).await.expect("get").expect("some").name, "a");
        assert!(uc.execute("ghost").await.expect("get").is_none());
    }
}
