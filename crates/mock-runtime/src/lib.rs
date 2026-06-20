//! mock-runtime —— Mock 运行时(请求匹配引擎)
//!
//! `api-definition` 只登记 Mock 的**定义**(期望响应),没有"被真实请求命中并返回"的运行时。
//! 本 crate 补上这一环:一个独立监听端口的服务,把进来的请求匹配到一条 Mock 规则并回放其响应。
//!
//! 设计:**匹配是纯函数**(domain,零 IO),给定请求视图 + 规则集,确定性选出命中规则——
//! 方法/路径(glob)/请求头/查询/请求体(包含 + JSON Pointer)逐项匹配,多条命中按 `priority`
//! 取最高、并列取先到。这样优先级、大小写、通配等易错点都能毫秒级穷举单测。
//! IO(从 PG/内存取规则、axum 兜底路由)留在 adapters,经 `ports::MockRuleSource` 解耦。

pub mod adapters;
pub mod domain;
pub mod ports;
