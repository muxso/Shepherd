use std::sync::Arc;

use crate::domain::{Decomposition, NewTask, TaskError, TaskStatus};
use crate::ports::{RepoError, TaskRepository};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskCmdError {
    DecompositionNotFound,
    TaskNotFound,
    Validation(TaskError),
    Conflict(TaskError),
    Repo(RepoError),
}

impl From<RepoError> for TaskCmdError {
    fn from(e: RepoError) -> Self {
        Self::Repo(e)
    }
}

impl From<TaskError> for TaskCmdError {
    fn from(e: TaskError) -> Self {
        match e {
            TaskError::NoSuchTask(_) => Self::TaskNotFound,
            TaskError::DependenciesNotSatisfied => {
                Self::Conflict(TaskError::DependenciesNotSatisfied)
            }
            TaskError::TransitionNotAllowed { from, to } => {
                Self::Conflict(TaskError::TransitionNotAllowed { from, to })
            }
            other => Self::Validation(other),
        }
    }
}

#[derive(Clone)]
pub struct TaskService {
    repo: Arc<dyn TaskRepository>,
}

impl TaskService {
    pub fn new(repo: Arc<dyn TaskRepository>) -> Self {
        Self { repo }
    }

    pub async fn get(&self, decomposition_id: &str) -> Result<Decomposition, TaskCmdError> {
        self.repo.get(decomposition_id).await?.ok_or(TaskCmdError::DecompositionNotFound)
    }

    pub async fn add_task(
        &self,
        decomposition_id: &str,
        title: &str,
        description: &str,
        acceptance_criteria: &[String],
        dependencies: &[String],
        points: i32,
    ) -> Result<String, TaskCmdError> {
        let mut d = self.get(decomposition_id).await?;
        let new = NewTask::new(title, description, acceptance_criteria, dependencies)?
            .with_points(points);
        let id = d.add_task(new)?;
        self.repo.save(&d).await?;
        Ok(id)
    }

    pub async fn set_points(
        &self,
        decomposition_id: &str,
        task_id: &str,
        points: i32,
    ) -> Result<Decomposition, TaskCmdError> {
        let mut d = self.get(decomposition_id).await?;
        d.set_points(task_id, points)?;
        self.repo.save(&d).await?;
        Ok(d)
    }

    pub async fn set_assignee(
        &self,
        decomposition_id: &str,
        task_id: &str,
        assignee: &str,
        kind: &str,
    ) -> Result<Decomposition, TaskCmdError> {
        let mut d = self.get(decomposition_id).await?;
        d.set_assignee(task_id, assignee, kind)?;
        self.repo.save(&d).await?;
        Ok(d)
    }

    /// Bulk reassignment; returns (updated decomposition, change count). Skips persisting when
    /// nothing changed.
    pub async fn reassign(
        &self,
        decomposition_id: &str,
        from: &str,
        to: &str,
        kind: &str,
    ) -> Result<(Decomposition, usize), TaskCmdError> {
        let mut d = self.get(decomposition_id).await?;
        let changed = d.reassign(from, to, kind);
        if changed > 0 {
            self.repo.save(&d).await?;
        }
        Ok((d, changed))
    }

    pub async fn dispatch(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Decomposition, TaskCmdError> {
        let mut d = self.get(decomposition_id).await?;
        d.dispatch(task_id)?;
        self.persist_status(&d, decomposition_id, task_id).await?;
        Ok(d)
    }

    pub async fn advance_to(
        &self,
        decomposition_id: &str,
        task_id: &str,
        target: TaskStatus,
    ) -> Result<Decomposition, TaskCmdError> {
        let mut d = self.get(decomposition_id).await?;
        d.advance_to(task_id, target)?;
        self.persist_status(&d, decomposition_id, task_id).await?;
        Ok(d)
    }

    pub async fn transition(
        &self,
        decomposition_id: &str,
        task_id: &str,
        to: TaskStatus,
    ) -> Result<Decomposition, TaskCmdError> {
        let mut d = self.get(decomposition_id).await?;
        d.transition(task_id, to)?;
        self.persist_status(&d, decomposition_id, task_id).await?;
        Ok(d)
    }

    /// Persist row-level, not whole-graph, so concurrent sibling advances don't lose updates.
    async fn persist_status(
        &self,
        d: &Decomposition,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<(), TaskCmdError> {
        let status = d
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .map(|t| t.status)
            .expect("task must exist after a successful advance");
        self.repo.save_task_status(decomposition_id, task_id, status).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryTaskRepository;
    use crate::application::CreateDecompositionUseCase;

    async fn seeded() -> (TaskService, String) {
        let repo = Arc::new(InMemoryTaskRepository::new());
        let id = CreateDecompositionUseCase::new(repo.clone())
            .execute("req1", 1)
            .await
            .expect("seed")
            .id;
        (TaskService::new(repo), id)
    }

    #[tokio::test]
    async fn add_dispatch_and_unlock_dependent() {
        let (svc, did) = seeded().await;
        let a = svc.add_task(&did, "A", "", &[], &[], 0).await.expect("a");
        let _b = svc.add_task(&did, "B", "", &[], std::slice::from_ref(&a), 0).await.expect("b");

        assert_eq!(
            svc.dispatch(&did, "t2").await.unwrap_err(),
            TaskCmdError::Conflict(TaskError::DependenciesNotSatisfied)
        );
        svc.dispatch(&did, &a).await.expect("dispatch a");
        svc.transition(&did, &a, TaskStatus::Running).await.expect("run");
        svc.transition(&did, &a, TaskStatus::Delivered).await.expect("deliver");
        svc.transition(&did, &a, TaskStatus::Verified).await.expect("verify");
        let d = svc.dispatch(&did, "t2").await.expect("dispatch b");
        assert_eq!(d.task("t2").expect("t2").status, TaskStatus::Dispatched);
    }

    #[tokio::test]
    async fn add_task_unknown_dependency_is_validation() {
        let (svc, did) = seeded().await;
        assert_eq!(
            svc.add_task(&did, "B", "", &[], &["t9".to_string()], 0).await.unwrap_err(),
            TaskCmdError::Validation(TaskError::UnknownDependency("t9".into()))
        );
    }

    #[tokio::test]
    async fn illegal_transition_is_conflict() {
        let (svc, did) = seeded().await;
        svc.add_task(&did, "A", "", &[], &[], 0).await.expect("a");
        assert_eq!(
            svc.transition(&did, "t1", TaskStatus::Verified).await.unwrap_err(),
            TaskCmdError::Conflict(TaskError::TransitionNotAllowed {
                from: "PENDING",
                to: "VERIFIED"
            })
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_sibling_advance_no_lost_update() {
        let (svc, did) = seeded().await;
        let svc = Arc::new(svc);
        let t1 = svc.add_task(&did, "root", "", &[], &[], 0).await.expect("t1");
        svc.add_task(&did, "left", "", &[], std::slice::from_ref(&t1), 0).await.expect("t2");
        svc.add_task(&did, "right", "", &[], std::slice::from_ref(&t1), 0).await.expect("t3");
        svc.advance_to(&did, &t1, TaskStatus::Verified).await.expect("t1 verified");

        let (s2, s3, d2, d3) = (svc.clone(), svc.clone(), did.clone(), did.clone());
        let h2 = tokio::spawn(async move { s2.advance_to(&d2, "t2", TaskStatus::Verified).await });
        let h3 = tokio::spawn(async move { s3.advance_to(&d3, "t3", TaskStatus::Verified).await });
        h2.await.expect("join t2").expect("t2 advanced");
        h3.await.expect("join t3").expect("t3 advanced");

        let d = svc.get(&did).await.expect("get");
        assert_eq!(
            d.task("t2").expect("t2").status,
            TaskStatus::Verified,
            "t2 should stay Verified"
        );
        assert_eq!(
            d.task("t3").expect("t3").status,
            TaskStatus::Verified,
            "t3 should not be overwritten by a sibling write-back"
        );
    }

    #[tokio::test]
    async fn missing_decomposition_and_task() {
        let (svc, did) = seeded().await;
        assert_eq!(svc.get("ghost").await.unwrap_err(), TaskCmdError::DecompositionNotFound);
        assert_eq!(svc.dispatch(&did, "t1").await.unwrap_err(), TaskCmdError::TaskNotFound);
    }
}
