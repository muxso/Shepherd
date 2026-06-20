//! 适配器层:并发引擎(tokio)+ 原生执行器(api-runner),均 feature 门控。
#[cfg(feature = "engine")]
pub mod engine;
#[cfg(feature = "engine")]
pub use engine::{run_collect, run_load};

pub mod in_memory_sink;
pub use in_memory_sink::InMemorySampleSink;

#[cfg(feature = "parquet-sink")]
pub mod parquet_sink;
#[cfg(feature = "parquet-sink")]
pub use parquet_sink::ParquetObjectStoreSink;

#[cfg(feature = "api-runner-exec")]
pub mod api_runner_exec;
#[cfg(feature = "api-runner-exec")]
pub use api_runner_exec::ApiRunnerExecutor;
