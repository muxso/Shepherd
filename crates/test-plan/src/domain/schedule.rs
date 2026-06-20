//! 计划定时调度 + 定时运行快照(零 IO)。
//!
//! 一个计划可配 cron;到点由调度器为计划**拍一份统计快照**(PlanRun)存档,从而看通过率/执行率随时间的趋势。

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduleError {
    #[error("plan id must not be empty")]
    EmptyPlan,
    #[error("cron expression must not be empty")]
    EmptyCron,
}

/// 已登记的定时计划。`cron` 为 6 段(秒 分 时 日 月 周)表达式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub id: String,
    pub plan_id: String,
    pub cron: String,
    pub enabled: bool,
}

/// 待登记的定时计划(构造即校验:计划/cron 非空)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSchedule {
    pub plan_id: String,
    pub cron: String,
    pub enabled: bool,
}

impl NewSchedule {
    pub fn new(plan_id: &str, cron: &str, enabled: bool) -> Result<Self, ScheduleError> {
        let plan_id = plan_id.trim();
        if plan_id.is_empty() {
            return Err(ScheduleError::EmptyPlan);
        }
        let cron = cron.trim();
        if cron.is_empty() {
            return Err(ScheduleError::EmptyCron);
        }
        Ok(Self { plan_id: plan_id.to_string(), cron: cron.to_string(), enabled })
    }
}

/// 一次定时运行的统计快照。
#[derive(Debug, Clone, PartialEq)]
pub struct PlanRun {
    pub id: String,
    pub plan_id: String,
    pub status: String,
    pub total: u64,
    pub pass_rate: f64,
    pub execute_rate: f64,
    pub triggered_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_trims() {
        let s = NewSchedule::new(" p1 ", " */5 * * * * * ", true).expect("ok");
        assert_eq!(s.plan_id, "p1");
        assert_eq!(s.cron, "*/5 * * * * *");
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(NewSchedule::new(" ", "* * * * * *", true).unwrap_err(), ScheduleError::EmptyPlan);
        assert_eq!(NewSchedule::new("p1", "  ", true).unwrap_err(), ScheduleError::EmptyCron);
    }
}
