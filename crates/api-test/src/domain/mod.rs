//! 领域层:批量运行配置 + 资源池解析 + 运行环境,零 IO。
pub mod batch_run;
pub mod environment;

pub use batch_run::{
    resolve_effective_pool, BatchRunCommand, BatchRunError, BatchRunMode, RetryConfig,
    RunModeConfig,
};
pub use environment::ResolvedEnv;
