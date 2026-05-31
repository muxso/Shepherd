//! 领域层:计划 + 执行统计,零 IO。
pub mod plan;
pub mod statistics;

pub use plan::{NewPlan, Plan, PlanError, PlanType, ROOT_GROUP};
pub use statistics::{status_by_children, CaseCounts, ExecStatus};
