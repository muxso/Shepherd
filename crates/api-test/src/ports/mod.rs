//! 端口层:资源池(读/管理)与批量执行器抽象 + 用例执行记录读端口。
pub mod batch_run_ports;
pub mod case_execution_ports;
pub mod resource_pool_admin;

pub use batch_run_ports::{
    BatchExecutorPort, DispatchOutcome, DispatchReport, DispatchSpec, EnvironmentPort, PortError,
    ResourcePoolPort, RunTask, TaskDispatcher,
};
pub use case_execution_ports::{CaseExecutionQueryPort, CaseExecutionRecord};
pub use resource_pool_admin::ResourcePoolAdminPort;
