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
