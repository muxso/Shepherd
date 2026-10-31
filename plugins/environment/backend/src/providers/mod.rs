//! Capability registration; each provider owns its protocol-specific adapter.
mod database;
mod redis;
pub fn registry() -> Result<plugin_runtime::PluginRegistry, plugin_runtime::PluginError> {
    let mut registry = database::registry()?;
    registry.register(std::sync::Arc::new(redis::RedisProvider::new()))?;
    Ok(registry)
}
