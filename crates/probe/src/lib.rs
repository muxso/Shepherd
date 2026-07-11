//! 多协议探测:ProbeRequest 经 ProtocolPlugin 执行得 RawProbe,再按 ProbeAssertion 求值出 ProbeOutcome。
//! 内置 http/grpc/sql/mysql/redis/websocket/ssh 插件,按 feature 注册进 PluginRegistry(见 default_registry)。
//! domain 求值为纯计算;协议 IO 全部在插件内。

pub mod domain;
pub mod plugins;
pub mod ports;
pub mod registry;

pub use domain::{evaluate, ProbeAssertion, ProbeOutcome, ProbeRequest, RawProbe};
pub use ports::ProtocolPlugin;
pub use registry::PluginRegistry;

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
    #[cfg(feature = "mysql")]
    {
        reg = reg.with(std::sync::Arc::new(plugins::MysqlPlugin::new()));
    }
    #[cfg(feature = "redis")]
    {
        reg = reg.with(std::sync::Arc::new(plugins::RedisPlugin::new()));
    }
    #[cfg(feature = "websocket")]
    {
        reg = reg.with(std::sync::Arc::new(plugins::WebSocketPlugin::new()));
    }
    #[cfg(feature = "ssh")]
    {
        reg = reg.with(std::sync::Arc::new(plugins::SshPlugin::new()));
    }
    reg
}
