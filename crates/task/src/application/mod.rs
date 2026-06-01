//! 应用层:任务拆分用例编排。依赖 `ports` trait,不碰具体 IO。
pub mod breakdown;
pub mod create_decomposition;
pub mod task_admin;

pub use breakdown::{BreakdownError, BreakdownUseCase};
pub use create_decomposition::{CreateDecompositionError, CreateDecompositionUseCase};
pub use task_admin::{TaskCmdError, TaskService};
