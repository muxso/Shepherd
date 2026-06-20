//! 适配器层:并发引擎(tokio)+ 原生执行器(api-runner),均 feature 门控。
#[cfg(feature = "engine")]
pub mod engine;
#[cfg(feature = "engine")]
pub use engine::run_load;

#[cfg(feature = "api-runner-exec")]
pub mod api_runner_exec;
#[cfg(feature = "api-runner-exec")]
pub use api_runner_exec::ApiRunnerExecutor;
