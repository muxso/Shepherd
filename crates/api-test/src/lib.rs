//! 接口测试执行编排:批量执行(BatchRun,串行/并行 + 重试 + 环境绑定)与资源池(ResourcePool)管理。
//! 派发经 TaskDispatcher/BatchExecutorPort 抽象;环境变量读写走 EnvironmentPort/EnvVarWriter。
//! 适配器按 feature 启用:local 进程内执行、jmeter 远程派发、pg/http、parquet-archive 报告归档。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
