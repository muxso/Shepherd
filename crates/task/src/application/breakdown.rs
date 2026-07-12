use std::sync::Arc;

use crate::domain::{Decomposition, NewTask, TaskError};
use crate::ports::{PlanError, Planner, RepoError, RequirementSpec, TaskRepository};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BreakdownError {
    EmptyRequirement,
    AlreadyExists,
    Plan(PlanError),
    Validation(TaskError),
    Repo(RepoError),
}

impl From<RepoError> for BreakdownError {
    fn from(e: RepoError) -> Self {
        Self::Repo(e)
    }
}
impl From<TaskError> for BreakdownError {
    fn from(e: TaskError) -> Self {
        Self::Validation(e)
    }
}

#[derive(Clone)]
pub struct BreakdownUseCase {
    repo: Arc<dyn TaskRepository>,
    planner: Arc<dyn Planner>,
}

impl BreakdownUseCase {
    pub fn new(repo: Arc<dyn TaskRepository>, planner: Arc<dyn Planner>) -> Self {
        Self { repo, planner }
    }

    pub async fn execute(&self, spec: &RequirementSpec) -> Result<Decomposition, BreakdownError> {
        if spec.requirement_id.trim().is_empty() {
            return Err(BreakdownError::EmptyRequirement);
        }
        if self
            .repo
            .find_by_requirement_version(&spec.requirement_id, spec.requirement_version)
            .await?
            .is_some()
        {
            return Err(BreakdownError::AlreadyExists);
        }

        let planned = self.planner.plan(spec).await.map_err(BreakdownError::Plan)?;

        let mut d =
            self.repo.create_decomposition(&spec.requirement_id, spec.requirement_version).await?;
        for (i, pt) in planned.iter().enumerate() {
            // Keep only back-references (j < i); a planner forward-reference would break the topological insert.
            let deps: Vec<String> = pt
                .dependencies
                .iter()
                .filter(|&&j| j < i)
                .map(|&j| format!("t{}", j + 1))
                .collect();
            let points = pt.acceptance_criteria.len().max(1) as i32;
            let nt = NewTask::new(&pt.title, &pt.description, &pt.acceptance_criteria, &deps)?
                .with_points(points);
            d.add_task(nt)?;
        }
        self.repo.save(&d).await?;
        Ok(d)
    }

    pub async fn find_existing(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Option<Decomposition>, RepoError> {
        self.repo.find_by_requirement_version(requirement_id, requirement_version).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{HeuristicPlanner, InMemoryTaskRepository};
    use crate::domain::TaskStatus;

    fn spec(criteria: &[&str]) -> RequirementSpec {
        RequirementSpec {
            requirement_id: "req1".into(),
            requirement_version: 1,
            title: "登录".into(),
            description: "d".into(),
            acceptance_criteria: criteria.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn uc() -> BreakdownUseCase {
        BreakdownUseCase::new(Arc::new(InMemoryTaskRepository::new()), Arc::new(HeuristicPlanner))
    }

    #[tokio::test]
    async fn breakdown_builds_dag_from_plan() {
        let d = uc().execute(&spec(&["登录成功", "错误密码拒绝"])).await.expect("breakdown");
        assert_eq!(d.tasks.len(), 3);
        assert_eq!(
            d.task("t3").expect("t3").dependencies,
            vec!["t1".to_string(), "t2".to_string()]
        );
        let ready: Vec<_> = d.ready_tasks().iter().map(|t| t.id.clone()).collect();
        assert_eq!(ready, vec!["t1".to_string(), "t2".to_string()]);
        assert!(d.tasks.iter().all(|t| t.status == TaskStatus::Pending));
    }

    #[tokio::test]
    async fn breakdown_is_idempotent_per_version() {
        let uc = uc();
        uc.execute(&spec(&["c1"])).await.expect("first");
        assert_eq!(uc.execute(&spec(&["c1"])).await.unwrap_err(), BreakdownError::AlreadyExists);
    }

    #[tokio::test]
    async fn rejects_empty_requirement() {
        let mut s = spec(&["c1"]);
        s.requirement_id = "  ".into();
        assert_eq!(uc().execute(&s).await.unwrap_err(), BreakdownError::EmptyRequirement);
    }
}
