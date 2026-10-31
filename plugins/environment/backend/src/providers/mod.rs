//! Capability registration; each provider owns its protocol-specific adapter.
mod database;
pub fn registry() -> Result<plugin_runtime::PluginRegistry, plugin_runtime::PluginError> {
    database::registry()
}
