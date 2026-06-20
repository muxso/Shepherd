//! perf —— 原生 Rust 压测引擎(NativePerfDispatcher 的核心)
//!
//! "不用 JMeter 的压测":domain 是零 IO 的统计聚合(延迟分位 / 吞吐 / 错误率),
//! `adapters::engine`(tokio)按负载计划并发施压并采样,经 `ports::RequestExecutor` 执行单次请求;
//! `adapters::api_runner_exec` 把该端口接到原生 reqwest 执行器(复用 `api-runner`),全程纯 Rust。
//!
//! 分层一如其余 context:domain/ports 不碰 IO(架构守卫兜底),并发与 HTTP 只在 adapters。
//! 后续可把它包成实现 `api-test::TaskDispatcher` 的 `NativePerfDispatcher`
//! (`DispatchOutcome::Accepted` 异步落 `ms_perf_report`),或加 worker 协议做分布式压测。

pub mod adapters;
pub mod domain;
pub mod ports;
