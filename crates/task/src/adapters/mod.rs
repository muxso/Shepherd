//! 适配器层。`in_memory` 纯 test double 始终编译;`pg`/`http` feature 门控。
pub mod in_memory;

pub mod heuristic_planner;

pub use heuristic_planner::HeuristicPlanner;
pub use in_memory::InMemoryTaskRepository;

#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "http")]
pub mod http;
