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
        self.execute_as(project_id, name, plan_type, group_id, None).await
    }

    /// Create with the acting user recorded as the plan creator.
    pub async fn execute_as(
        &self,
        project_id: &str,
        name: &str,
        plan_type: PlanType,
        group_id: &str,
        created_by: Option<&str>,
    ) -> Result<Plan, CreatePlanError> {
        let mut new_plan = NewPlan::new(project_id, name, plan_type, group_id)?;
        if let Some(user) = created_by {
            new_plan = new_plan.with_created_by(user);
        }

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
        let p = uc.execute("proj1", "smoke", PlanType::Plan, ROOT_GROUP).await.expect("ok");
        assert_eq!(p.name, "smoke");
        assert!(!p.archived);
    }

    #[tokio::test]
    async fn creates_plan_with_creator() {
        let repo = Arc::new(InMemoryPlanRepository::new());
        let uc = CreatePlanUseCase::new(repo.clone());
        let p = uc
            .execute_as("proj1", "smoke", PlanType::Plan, ROOT_GROUP, Some("admin"))
            .await
            .expect("ok");
        assert_eq!(p.created_by.as_deref(), Some("admin"));
        let list = repo.list("proj1").await.expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0.created_by.as_deref(), Some("admin"));
        // No user: created_by stays empty.
        let anon = uc.execute("proj1", "anonymous", PlanType::Plan, ROOT_GROUP).await.expect("ok");
        assert_eq!(anon.created_by, None);
    }

    #[tokio::test]
    async fn nested_group_rejected_by_domain() {
        let uc = CreatePlanUseCase::new(Arc::new(InMemoryPlanRepository::new()));
        let err = uc.execute("proj1", "nested group", PlanType::Group, "g1").await.unwrap_err();
        assert_eq!(err, CreatePlanError::Validation(PlanError::GroupCannotBeNested));
    }

    #[tokio::test]
    async fn plan_in_existing_group_ok() {
        let repo = InMemoryPlanRepository::new();
        let uc = CreatePlanUseCase::new(Arc::new(repo.clone()));
        let group = uc
            .execute("proj1", "regression group", PlanType::Group, ROOT_GROUP)
            .await
            .expect("group");

        let child =
            uc.execute("proj1", "child plan", PlanType::Plan, &group.id).await.expect("child");
        assert_eq!(child.group_id, group.id);
    }

    #[tokio::test]
    async fn plan_in_missing_group_rejected() {
        let uc = CreatePlanUseCase::new(Arc::new(InMemoryPlanRepository::new()));
        let err = uc.execute("proj1", "orphan", PlanType::Plan, "ghost").await.unwrap_err();
        assert_eq!(err, CreatePlanError::InvalidGroup);
    }

    #[tokio::test]
    async fn plan_parented_to_a_non_group_rejected() {
        let repo = InMemoryPlanRepository::new();
        let uc = CreatePlanUseCase::new(Arc::new(repo.clone()));
        let plain = uc.execute("proj1", "plain", PlanType::Plan, ROOT_GROUP).await.expect("plan");
        let err = uc.execute("proj1", "child", PlanType::Plan, &plain.id).await.unwrap_err();
        assert_eq!(err, CreatePlanError::InvalidGroup);
    }
}
