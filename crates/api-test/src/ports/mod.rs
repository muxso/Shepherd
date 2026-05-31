//! 端口层:资源池与批量执行器抽象。
pub mod batch_run_ports;

pub use batch_run_ports::{
    BatchExecutorPort, DispatchOutcome, DispatchSpec, PortError, ResourcePoolPort, RunTask,
    TaskDispatcher,
};
