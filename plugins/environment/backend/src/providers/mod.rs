//! Capability registration; each provider owns its protocol-specific adapter.
mod database;
mod grpc;
mod host;
pub(crate) mod javascript;
mod redis;
mod setup;
mod ssh;
pub fn registry() -> Result<plugin_runtime::PluginRegistry, plugin_runtime::PluginError> {
    let mut registry = database::registry()?;
    registry.register(std::sync::Arc::new(redis::RedisProvider::new()))?;
    registry.register(std::sync::Arc::new(ssh::SshProvider::new()))?;
    registry.register(std::sync::Arc::new(grpc::GrpcProvider::new()))?;
    registry.register(std::sync::Arc::new(host::HostProvider))?;
    registry.register(std::sync::Arc::new(javascript::JavascriptProvider))?;
    registry.register(std::sync::Arc::new(setup::SetupProvider))?;
    Ok(registry)
}
