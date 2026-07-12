//! API test execution orchestration: batch runs (BatchRun; serial/parallel + retry + environment binding) and resource pool (ResourcePool) management.
//! Dispatch goes through the TaskDispatcher/BatchExecutorPort abstractions; env var reads/writes go through EnvironmentPort/EnvVarWriter.
//! Adapters are feature-gated: local in-process execution, jmeter remote dispatch, pg/http, parquet-archive report archiving.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
