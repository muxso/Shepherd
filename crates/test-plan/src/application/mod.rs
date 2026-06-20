//! 应用层:计划用例编排。
pub mod create_plan;
pub mod plan_cases;
pub mod plan_statistics;
pub mod report;
pub mod scheduling;

pub use create_plan::{CreatePlanError, CreatePlanUseCase};
pub use plan_cases::PlanCaseUseCase;
pub use plan_statistics::{PlanStatistics, PlanStatisticsError, PlanStatisticsUseCase};
pub use report::{report_html, report_markdown};
pub use scheduling::{
    CreateScheduleError, CreateScheduleUseCase, ScheduledRunError, ScheduledRunUseCase,
};
