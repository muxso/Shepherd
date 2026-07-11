//! 压测上下文:LoadPlan 定义负载(按迭代次数或持续时长),engine 按计划驱动 RequestExecutor 发压
//! (api-runner-exec/probe-exec 分别复用 api-runner 与 probe),
//! Sample 经 SampleSink(内存/parquet-sink)落盘并汇总为 LatencyStats/LoadReport。
//! domain/ports 不做 IO;各执行器与 sink 适配器由 feature 启用。

pub mod adapters;
pub mod domain;
pub mod ports;
