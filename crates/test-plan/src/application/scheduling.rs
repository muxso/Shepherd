use std::sync::Arc;

use thiserror::Error;

use crate::application::{PlanStatisticsError, PlanStatisticsUseCase};
use crate::domain::{NewSchedule, PlanRun, Schedule, ScheduleError};
use crate::ports::{PlanRunStore, RepoError, ScheduleStore};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateScheduleError {
    #[error(transparent)]
    Validation(#[from] ScheduleError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateScheduleUseCase {
    schedules: Arc<dyn ScheduleStore>,
}

impl CreateScheduleUseCase {
    pub fn new(schedules: Arc<dyn ScheduleStore>) -> Self {
        Self { schedules }
    }

    pub async fn execute(
        &self,
        plan_id: &str,
        cron: &str,
        enabled: bool,
    ) -> Result<Schedule, CreateScheduleError> {
        let new = NewSchedule::new(plan_id, cron, enabled)?;
        Ok(self.schedules.insert(&new).await?)
    }

    pub async fn list_enabled(&self) -> Result<Vec<Schedule>, RepoError> {
        self.schedules.list_enabled().await
    }

    pub async fn delete(&self, plan_id: &str) -> Result<bool, RepoError> {
        self.schedules.delete_by_plan(plan_id).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduledRunError {
    #[error(transparent)]
    Stats(#[from] PlanStatisticsError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct ScheduledRunUseCase {
    stats: PlanStatisticsUseCase,
    runs: Arc<dyn PlanRunStore>,
}

impl ScheduledRunUseCase {
    pub fn new(stats: PlanStatisticsUseCase, runs: Arc<dyn PlanRunStore>) -> Self {
        Self { stats, runs }
    }

    pub async fn execute(&self, plan_id: &str) -> Result<PlanRun, ScheduledRunError> {
        let s = self.stats.execute(plan_id).await?;
        Ok(self
            .runs
            .record(plan_id, s.status.as_str(), s.total, s.pass_rate, s.execute_rate)
            .await?)
    }

    pub async fn list_runs(&self, plan_id: &str, limit: u32) -> Result<Vec<PlanRun>, RepoError> {
        self.runs.list_by_plan(plan_id, limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{InMemoryPlanRepository, InMemoryScheduleStore};
    use crate::domain::{CaseCounts, NewPlan, PlanType, ROOT_GROUP};

    #[tokio::test]
    async fn create_and_list_schedule() {
        let store = Arc::new(InMemoryScheduleStore::new());
        let uc = CreateScheduleUseCase::new(store);
        uc.execute("p1", "*/5 * * * * *", true).await.expect("create");
        assert_eq!(uc.list_enabled().await.expect("list").len(), 1);
    }

    #[tokio::test]
    async fn resave_replaces_and_delete_removes() {
        let store = Arc::new(InMemoryScheduleStore::new());
        let uc = CreateScheduleUseCase::new(store);
        uc.execute("p1", "*/5 * * * * *", true).await.expect("create");
        uc.execute("p1", "0 0 * * * *", true).await.expect("resave");
        let enabled = uc.list_enabled().await.expect("list");
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].cron, "0 0 * * * *");
        assert!(uc.delete("p1").await.expect("delete"));
        assert!(uc.list_enabled().await.expect("list").is_empty());
        assert!(!uc.delete("p1").await.expect("second delete"));
    }

    #[tokio::test]
    async fn scheduled_run_snapshots_stats() {
        let repo = InMemoryPlanRepository::new();
        let plan =
            repo.seed(NewPlan::new("p1", "冒烟", PlanType::Plan, ROOT_GROUP).expect("v")).await;
        repo.set_counts(&plan.id, CaseCounts { success: 3, error: 1, ..Default::default() });
        let store = Arc::new(InMemoryScheduleStore::new());
        let stats = PlanStatisticsUseCase::new(Arc::new(repo));
        let run_uc = ScheduledRunUseCase::new(stats, store);

        let run = run_uc.execute(&plan.id).await.expect("run");
        assert_eq!(run.total, 4);
        assert!(run.pass_rate > 0.0);
        run_uc.execute(&plan.id).await.expect("run2");
        assert_eq!(run_uc.list_runs(&plan.id, 10).await.expect("list").len(), 2);
    }
}
