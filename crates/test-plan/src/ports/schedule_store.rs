//! 定时调度 + 运行快照仓储端口。

use async_trait::async_trait;

use crate::domain::{NewSchedule, PlanRun, Schedule};
use crate::ports::RepoError;

/// 定时计划注册表。
#[async_trait]
pub trait ScheduleStore: Send + Sync {
    async fn insert(&self, s: &NewSchedule) -> Result<Schedule, RepoError>;
    /// 列出全部启用的计划(供调度器启动时加载)。
    async fn list_enabled(&self) -> Result<Vec<Schedule>, RepoError>;
}

/// 定时运行快照存档。
#[async_trait]
pub trait PlanRunStore: Send + Sync {
    /// 记录一次定时运行的统计快照,返回入档记录。
    async fn record(
        &self,
        plan_id: &str,
        status: &str,
        total: u64,
        pass_rate: f64,
        execute_rate: f64,
    ) -> Result<PlanRun, RepoError>;

    /// 某计划最近的运行快照(时间倒序)。
    async fn list_by_plan(&self, plan_id: &str, limit: u32) -> Result<Vec<PlanRun>, RepoError>;
}
