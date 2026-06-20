//! 应用层:批量运行用例编排 + 用例执行记录分页查询 + 资源池管理。
pub mod list_case_executions;
pub mod manage_resource_pool;
pub mod start_batch_run;

pub use list_case_executions::ListCaseExecutionsUseCase;
pub use manage_resource_pool::{
    CreateResourcePoolError, CreateResourcePoolUseCase, ListResourcePoolsUseCase,
};
pub use start_batch_run::StartBatchRunUseCase;
