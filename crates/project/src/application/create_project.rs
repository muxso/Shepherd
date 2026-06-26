use std::sync::Arc;

use thiserror::Error;

use crate::domain::{NewProject, Project, ProjectError};
use crate::ports::{ProjectRepository, RepoError};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateProjectError {
    #[error(transparent)]
    Validation(#[from] ProjectError),
    #[error("project name already exists")]
    NameAlreadyExists,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateProjectUseCase {
    repo: Arc<dyn ProjectRepository>,
}

impl CreateProjectUseCase {
    pub fn new(repo: Arc<dyn ProjectRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        organization_id: &str,
        name: &str,
    ) -> Result<Project, CreateProjectError> {
        let new_project = NewProject::new(organization_id, name)?;

        if self
            .repo
            .find_active_by_name(&new_project.organization_id, &new_project.name)
            .await?
            .is_some()
        {
            return Err(CreateProjectError::NameAlreadyExists);
        }

        Ok(self.repo.insert(&new_project).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryProjectRepository;

    #[tokio::test]
    async fn creates_project_with_unique_name() {
        let uc = CreateProjectUseCase::new(Arc::new(InMemoryProjectRepository::new()));
        let p = uc.execute("org1", "Demo").await.expect("create ok");
        assert_eq!(p.name, "Demo");
        assert_eq!(p.organization_id, "org1");
        assert!(p.enable && !p.deleted);
    }

    #[tokio::test]
    async fn rejects_duplicate_name_in_same_org() {
        let uc = CreateProjectUseCase::new(Arc::new(InMemoryProjectRepository::new()));
        uc.execute("org1", "Demo").await.expect("first ok");
        let err = uc.execute("org1", "Demo").await.unwrap_err();
        assert_eq!(err, CreateProjectError::NameAlreadyExists);
    }

    #[tokio::test]
    async fn allows_same_name_in_different_org() {
        let uc = CreateProjectUseCase::new(Arc::new(InMemoryProjectRepository::new()));
        uc.execute("org1", "Demo").await.expect("ok");
        assert!(uc.execute("org2", "Demo").await.is_ok());
    }

    #[tokio::test]
    async fn allows_recreating_name_freed_by_soft_delete() {
        let repo = InMemoryProjectRepository::new();
        let uc = CreateProjectUseCase::new(Arc::new(repo.clone()));
        let p = uc.execute("org1", "Demo").await.expect("ok");

        repo.soft_delete(&p.id);
        assert!(uc.execute("org1", "Demo").await.is_ok());
    }

    #[tokio::test]
    async fn propagates_validation_error() {
        let uc = CreateProjectUseCase::new(Arc::new(InMemoryProjectRepository::new()));
        let err = uc.execute("org1", "  ").await.unwrap_err();
        assert_eq!(err, CreateProjectError::Validation(ProjectError::EmptyName));
    }
}
