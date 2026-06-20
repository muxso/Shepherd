//! probe —— 协议探测插件系统
//!
//! 把「执行一次协议探测」抽象成插件:`ProtocolPlugin` 按协议执行,产出 `RawProbe`;
//! 通用断言(`ProbeAssertion`,纯函数)对输出判定,合成 `ProbeOutcome`。
//! `PluginRegistry` 按协议名分发。每种协议是一个 **feature 门控**的插件
//! (http 默认;grpc/sql 等按需)——runner 按启用的 feature 决定支持哪些系统,无需改核心。
//!
//! 用 `default_registry()` 拿到当前构建启用的全部插件组成的注册表。

pub mod domain;
pub mod plugins;
pub mod ports;
pub mod registry;

pub use domain::{evaluate, ProbeAssertion, ProbeOutcome, ProbeRequest, RawProbe};
pub use ports::ProtocolPlugin;
pub use registry::PluginRegistry;

/// 据启用的 feature 组装默认插件注册表(http 默认在;grpc/sql 视 feature 而定)。
/// 无任何插件 feature 时(如仅取协议类型的下游)返回空注册表。
pub fn default_registry() -> PluginRegistry {
    #[allow(unused_mut)]
    let mut reg = PluginRegistry::new();
    #[cfg(feature = "http")]
    {
        reg = reg.with(std::sync::Arc::new(plugins::HttpPlugin::new()));
    }
    #[cfg(feature = "grpc")]
    {
        reg = reg.with(std::sync::Arc::new(plugins::GrpcPlugin::new()));
    }
    #[cfg(feature = "sql")]
    {
        reg = reg.with(std::sync::Arc::new(plugins::SqlPlugin::new()));
    }
    #[cfg(feature = "redis")]
    {
        reg = reg.with(std::sync::Arc::new(plugins::RedisPlugin::new()));
    }
    reg
}
