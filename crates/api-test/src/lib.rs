//! api-test —— 阶段 6,接口测试(批量执行)
//!
//! 本轮把一条被记录在案的真实 quirk 提炼成领域用例:
//! **批量运行的资源池解析**。Java 版 `getGetResourcePoolNodeDTO` 的逻辑是:
//! 客户端 `poolId` 优先,空则回退项目默认池,再查可用性,查不到才抛错——
//! 但很多部署项目默认池为空,导致回退后查不到池,**深在执行链里 500**。
//!
//! 这里把"池解析优先级 + 必须可用"做成零 IO 的纯规则 + 用例:
//! 解析不出可用池时,在入口就给出**明确的领域错误**,而不是下游崩。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
