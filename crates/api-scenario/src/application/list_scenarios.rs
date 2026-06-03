//! 用例:列出某项目的场景(排除已删除,steps 已加载)。

use std::sync::Arc;

use crate::domain::ApiScenario;
use crate::ports::{ApiScenarioRepository, RepoError};

#[derive(Clone)]
pub struct ListScenariosUseCase {
    repo: Arc<dyn ApiScenarioRepository>,
}

impl ListScenariosUseCase {
    pub fn new(repo: Arc<dyn ApiScenarioRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, project_id: &str) -> Result<Vec<ApiScenario>, RepoError> {
        self.repo.list_scenarios(project_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiScenarioRepository;
    use crate::application::CreateScenarioUseCase;

    #[tokio::test]
    async fn lists_scenarios_of_project() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let create = CreateScenarioUseCase::new(repo.clone());
        create.execute("p1", "a").await.expect("a");
        create.execute("p1", "b").await.expect("b");
        create.execute("p2", "c").await.expect("c");

        let uc = ListScenariosUseCase::new(repo);
        let list = uc.execute("p1").await.expect("list");
        assert_eq!(list.len(), 2);
        assert!(uc.execute("p3").await.expect("empty").is_empty());
    }
}
