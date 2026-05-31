//! 用例:拆分图内的任务编排 —— get / add_task / dispatch / transition。
//!
//! 领域不变量(依赖存在、就绪门控、状态机)在 `Decomposition` 聚合里;本服务负责
//! 加载-变更-落库,并把领域错误翻译成带语义的命令错误(供 HTTP 层映射到 404/409/400)。

use std::sync::Arc;

use crate::domain::{Decomposition, NewTask, TaskError, TaskStatus};
use crate::ports::{RepoError, TaskRepository};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskCmdError {
    /// 拆分图不存在 → 404。
    DecompositionNotFound,
    /// 任务不存在 → 404。
    TaskNotFound,
    /// 入参/校验失败(空标题、未知依赖等)→ 400。
    Validation(TaskError),
    /// 冲突(依赖未满足、非法状态流转)→ 409。
    Conflict(TaskError),
    /// 存储错误 → 500。
    Repo(RepoError),
}

impl From<RepoError> for TaskCmdError {
    fn from(e: RepoError) -> Self {
        Self::Repo(e)
    }
}

// 领域错误 → 命令错误的确定性映射。
impl From<TaskError> for TaskCmdError {
    fn from(e: TaskError) -> Self {
        match e {
            TaskError::NoSuchTask(_) => Self::TaskNotFound,
            TaskError::DependenciesNotSatisfied => Self::Conflict(TaskError::DependenciesNotSatisfied),
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

    /// 向拆分图加入一个任务,返回新任务的本地 id。
    pub async fn add_task(
        &self,
        decomposition_id: &str,
        title: &str,
        description: &str,
        acceptance_criteria: &[String],
        dependencies: &[String],
    ) -> Result<String, TaskCmdError> {
        let mut d = self.get(decomposition_id).await?;
        let new = NewTask::new(title, description, acceptance_criteria, dependencies)?;
        let id = d.add_task(new)?;
        self.repo.save(&d).await?;
        Ok(id)
    }

    /// 派发任务(Pending→Dispatched,依赖须全部 Verified)。
    pub async fn dispatch(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Decomposition, TaskCmdError> {
        let mut d = self.get(decomposition_id).await?;
        d.dispatch(task_id)?;
        self.repo.save(&d).await?;
        Ok(d)
    }

    /// 状态流转(Running/Delivered/Verified/Failed/重试 Pending)。
    pub async fn transition(
        &self,
        decomposition_id: &str,
        task_id: &str,
        to: TaskStatus,
    ) -> Result<Decomposition, TaskCmdError> {
        let mut d = self.get(decomposition_id).await?;
        d.transition(task_id, to)?;
        self.repo.save(&d).await?;
        Ok(d)
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
        let a = svc.add_task(&did, "A", "", &[], &[]).await.expect("a");
        let _b = svc.add_task(&did, "B", "", &[], &[a.clone()]).await.expect("b");

        // B 依赖未满足
        assert_eq!(
            svc.dispatch(&did, "t2").await.unwrap_err(),
            TaskCmdError::Conflict(TaskError::DependenciesNotSatisfied)
        );
        // 驱动 A 到 Verified
        svc.dispatch(&did, &a).await.expect("dispatch a");
        svc.transition(&did, &a, TaskStatus::Running).await.expect("run");
        svc.transition(&did, &a, TaskStatus::Delivered).await.expect("deliver");
        svc.transition(&did, &a, TaskStatus::Verified).await.expect("verify");
        // 现在 B 可派发,落库可见
        let d = svc.dispatch(&did, "t2").await.expect("dispatch b");
        assert_eq!(d.task("t2").expect("t2").status, TaskStatus::Dispatched);
    }

    #[tokio::test]
    async fn add_task_unknown_dependency_is_validation() {
        let (svc, did) = seeded().await;
        assert_eq!(
            svc.add_task(&did, "B", "", &[], &["t9".to_string()]).await.unwrap_err(),
            TaskCmdError::Validation(TaskError::UnknownDependency("t9".into()))
        );
    }

    #[tokio::test]
    async fn illegal_transition_is_conflict() {
        let (svc, did) = seeded().await;
        svc.add_task(&did, "A", "", &[], &[]).await.expect("a");
        assert_eq!(
            svc.transition(&did, "t1", TaskStatus::Verified).await.unwrap_err(),
            TaskCmdError::Conflict(TaskError::TransitionNotAllowed { from: "PENDING", to: "VERIFIED" })
        );
    }

    #[tokio::test]
    async fn missing_decomposition_and_task() {
        let (svc, did) = seeded().await;
        assert_eq!(svc.get("ghost").await.unwrap_err(), TaskCmdError::DecompositionNotFound);
        assert_eq!(
            svc.dispatch(&did, "t1").await.unwrap_err(),
            TaskCmdError::TaskNotFound
        );
    }
}
