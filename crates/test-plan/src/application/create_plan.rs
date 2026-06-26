use std::sync::Arc;

use crate::domain::{NewPlan, Plan, PlanError, PlanType};
use crate::ports::{PlanRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreatePlanError {
    #[error(transparent)]
    Validation(#[from] PlanError),
    #[error("group not found or not a group")]
    InvalidGroup,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreatePlanUseCase {
    repo: Arc<dyn PlanRepository>,
}

impl CreatePlanUseCase {
    pub fn new(repo: Arc<dyn PlanRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        project_id: &str,
        name: &str,
        plan_type: PlanType,
        group_id: &str,
    ) -> Result<Plan, CreatePlanError> {
        let new_plan = NewPlan::new(project_id, name, plan_type, group_id)?;

        if new_plan.belongs_to_group() {
            match self.repo.get(&new_plan.group_id).await? {
                Some(parent) if parent.plan_type == PlanType::Group => {}
                _ => return Err(CreatePlanError::InvalidGroup),
            }
        }

        Ok(self.repo.insert(&new_plan).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryPlanRepository;
    use crate::domain::ROOT_GROUP;

    #[tokio::test]
    async fn creates_root_plan() {
        let uc = CreatePlanUseCase::new(Arc::new(InMemoryPlanRepository::new()));
        let p = uc.execute("proj1", "冒烟", PlanType::Plan, ROOT_GROUP).await.expect("ok");
        assert_eq!(p.name, "冒烟");
        assert!(!p.archived);
    }

    #[tokio::test]
    async fn nested_group_rejected_by_domain() {
        let uc = CreatePlanUseCase::new(Arc::new(InMemoryPlanRepository::new()));
        let err = uc.execute("proj1", "组中组", PlanType::Group, "g1").await.unwrap_err();
        assert_eq!(err, CreatePlanError::Validation(PlanError::GroupCannotBeNested));
    }

    #[tokio::test]
    async fn plan_in_existing_group_ok() {
        let repo = InMemoryPlanRepository::new();
        let uc = CreatePlanUseCase::new(Arc::new(repo.clone()));
        let group = uc.execute("proj1", "回归组", PlanType::Group, ROOT_GROUP).await.expect("group");

        let child = uc.execute("proj1", "子计划", PlanType::Plan, &group.id).await.expect("child");
        assert_eq!(child.group_id, group.id);
    }

    #[tokio::test]
    async fn plan_in_missing_group_rejected() {
        let uc = CreatePlanUseCase::new(Arc::new(InMemoryPlanRepository::new()));
        let err = uc.execute("proj1", "孤儿", PlanType::Plan, "ghost").await.unwrap_err();
        assert_eq!(err, CreatePlanError::InvalidGroup);
    }

    #[tokio::test]
    async fn plan_parented_to_a_non_group_rejected() {
        let repo = InMemoryPlanRepository::new();
        let uc = CreatePlanUseCase::new(Arc::new(repo.clone()));
        let plain = uc.execute("proj1", "普通", PlanType::Plan, ROOT_GROUP).await.expect("plan");
        let err = uc.execute("proj1", "子", PlanType::Plan, &plain.id).await.unwrap_err();
        assert_eq!(err, CreatePlanError::InvalidGroup);
    }
}
