//! 端口层:资源池与批量执行器抽象 + 用例执行记录读端口。
pub mod batch_run_ports;
pub mod case_execution_ports;

pub use batch_run_ports::{
    BatchExecutorPort, DispatchOutcome, DispatchReport, DispatchSpec, EnvironmentPort, PortError,
    ResourcePoolPort, RunTask, TaskDispatcher,
};
pub use case_execution_ports::{CaseExecutionQueryPort, CaseExecutionRecord};
