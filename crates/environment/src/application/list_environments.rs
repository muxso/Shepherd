use std::sync::Arc;

use crate::domain::Environment;
use crate::ports::{EnvironmentRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ListEnvironmentsError {
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct ListEnvironmentsUseCase {
    repo: Arc<dyn EnvironmentRepository>,
}

impl ListEnvironmentsUseCase {
    pub fn new(repo: Arc<dyn EnvironmentRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, project_id: &str) -> Result<Vec<Environment>, ListEnvironmentsError> {
        Ok(self.repo.list_by_project(project_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryEnvironmentRepository;
    use crate::application::{CreateEnvironmentUseCase, EnvironmentInput};
    use std::collections::BTreeMap;

    fn input(project: &str, name: &str) -> EnvironmentInput {
        EnvironmentInput {
            project_id: project.into(),
            name: name.into(),
            base_url: "".into(),
            headers: vec![],
            variables: BTreeMap::new(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn lists_only_project_environments() {
        let repo = Arc::new(InMemoryEnvironmentRepository::new());
        let create = CreateEnvironmentUseCase::new(repo.clone());
        create.execute(input("p1", "a")).await.expect("ok");
        create.execute(input("p1", "b")).await.expect("ok");
        create.execute(input("p2", "c")).await.expect("ok");

        let uc = ListEnvironmentsUseCase::new(repo);
        assert_eq!(uc.execute("p1").await.expect("ok").len(), 2);
    }
}
