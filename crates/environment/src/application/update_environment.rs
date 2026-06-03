//! 用例:更新环境。

use std::sync::Arc;

use crate::application::EnvironmentInput;
use crate::domain::{Environment, EnvironmentError, NewEnvironment};
use crate::ports::{EnvironmentRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UpdateEnvironmentError {
    #[error(transparent)]
    Validation(#[from] EnvironmentError),
    #[error(transparent)]
    Repo(#[from] RepoError),
    #[error("environment not found")]
    NotFound,
}

#[derive(Clone)]
pub struct UpdateEnvironmentUseCase {
    repo: Arc<dyn EnvironmentRepository>,
}

impl UpdateEnvironmentUseCase {
    pub fn new(repo: Arc<dyn EnvironmentRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        id: &str,
        input: EnvironmentInput,
    ) -> Result<Environment, UpdateEnvironmentError> {
        let new_env = NewEnvironment::new(
            &input.project_id,
            &input.name,
            &input.base_url,
            input.headers,
            input.variables,
            input.enabled,
        )?;
        self.repo
            .update(id, &new_env)
            .await?
            .ok_or(UpdateEnvironmentError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryEnvironmentRepository;
    use crate::application::CreateEnvironmentUseCase;
    use std::collections::BTreeMap;

    fn input(name: &str) -> EnvironmentInput {
        EnvironmentInput {
            project_id: "p1".into(),
            name: name.into(),
            base_url: "".into(),
            headers: vec![],
            variables: BTreeMap::new(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn updates_existing() {
        let repo = Arc::new(InMemoryEnvironmentRepository::new());
        let created = CreateEnvironmentUseCase::new(repo.clone())
            .execute(input("old"))
            .await
            .expect("ok");
        let uc = UpdateEnvironmentUseCase::new(repo);
        let updated = uc.execute(&created.id, input("new")).await.expect("ok");
        assert_eq!(updated.name, "new");
        assert_eq!(updated.id, created.id);
    }

    #[tokio::test]
    async fn missing_is_not_found() {
        let repo = Arc::new(InMemoryEnvironmentRepository::new());
        let uc = UpdateEnvironmentUseCase::new(repo);
        assert_eq!(uc.execute("ghost", input("x")).await.unwrap_err(), UpdateEnvironmentError::NotFound);
    }
}
