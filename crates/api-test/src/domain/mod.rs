//! 领域层:批量运行配置 + 资源池解析,零 IO。
pub mod batch_run;

pub use batch_run::{
    resolve_effective_pool, BatchRunCommand, BatchRunError, BatchRunMode, RetryConfig,
    RunModeConfig,
};
