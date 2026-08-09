use std::sync::Arc;

use crate::domain::Plan;
use crate::ports::{PlanRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManagePlanError {
    #[error("plan name must not be empty")]
    EmptyName,
    #[error("plan not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct ManagePlanUseCase {
    repo: Arc<dyn PlanRepository>,
}

impl ManagePlanUseCase {
    pub fn new(repo: Arc<dyn PlanRepository>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, project_id: &str) -> Result<Vec<Plan>, ManagePlanError> {
        Ok(self.repo.list(project_id).await?)
    }

    pub async fn rename(&self, id: &str, name: &str) -> Result<(), ManagePlanError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ManagePlanError::EmptyName);
        }
        if self.repo.rename(id, name).await? {
            Ok(())
        } else {
            Err(ManagePlanError::NotFound)
        }
    }

    pub async fn delete(&self, id: &str) -> Result<(), ManagePlanError> {
        if self.repo.delete(id).await? {
            Ok(())
        } else {
            Err(ManagePlanError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryPlanRepository;
    use crate::domain::{NewPlan, PlanType, ROOT_GROUP};

    async fn seed(repo: &InMemoryPlanRepository, project: &str, name: &str) -> Plan {
        repo.seed(NewPlan::new(project, name, PlanType::Plan, ROOT_GROUP).expect("v")).await
    }

    #[tokio::test]
    async fn list_scoped_to_project() {
        let repo = Arc::new(InMemoryPlanRepository::new());
        seed(&repo, "p1", "smoke").await;
        seed(&repo, "p1", "regression").await;
        seed(&repo, "p2", "another project").await;
        let uc = ManagePlanUseCase::new(repo);
        let list = uc.list("p1").await.expect("list");
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|p| p.project_id == "p1"));
    }

    #[tokio::test]
    async fn rename_then_list_reflects() {
        let repo = Arc::new(InMemoryPlanRepository::new());
        let p = seed(&repo, "p1", "old name").await;
        let uc = ManagePlanUseCase::new(repo);
        uc.rename(&p.id, "new name").await.expect("rename");
        let list = uc.list("p1").await.expect("list");
        assert_eq!(list[0].name, "new name");
    }

    #[tokio::test]
    async fn rename_blank_rejected() {
        let repo = Arc::new(InMemoryPlanRepository::new());
        let p = seed(&repo, "p1", "x").await;
        let uc = ManagePlanUseCase::new(repo);
        assert_eq!(uc.rename(&p.id, "  ").await.unwrap_err(), ManagePlanError::EmptyName);
    }

    #[tokio::test]
    async fn rename_missing_not_found() {
        let uc = ManagePlanUseCase::new(Arc::new(InMemoryPlanRepository::new()));
        assert_eq!(uc.rename("ghost", "x").await.unwrap_err(), ManagePlanError::NotFound);
    }

    #[tokio::test]
    async fn delete_removes_from_list() {
        let repo = Arc::new(InMemoryPlanRepository::new());
        let p = seed(&repo, "p1", "to delete").await;
        let uc = ManagePlanUseCase::new(repo);
        uc.delete(&p.id).await.expect("delete");
        assert!(uc.list("p1").await.expect("list").is_empty());
        assert_eq!(uc.delete(&p.id).await.unwrap_err(), ManagePlanError::NotFound);
    }
}
