use async_trait::async_trait;

use crate::domain::{NewSchedule, PlanRun, Schedule};
use crate::ports::RepoError;

#[async_trait]
pub trait ScheduleStore: Send + Sync {
    async fn insert(&self, s: &NewSchedule) -> Result<Schedule, RepoError>;
    async fn list_enabled(&self) -> Result<Vec<Schedule>, RepoError>;
}

#[async_trait]
pub trait PlanRunStore: Send + Sync {
    async fn record(
        &self,
        plan_id: &str,
        status: &str,
        total: u64,
        pass_rate: f64,
        execute_rate: f64,
    ) -> Result<PlanRun, RepoError>;

    /// 时间倒序。
    async fn list_by_plan(&self, plan_id: &str, limit: u32) -> Result<Vec<PlanRun>, RepoError>;
}
