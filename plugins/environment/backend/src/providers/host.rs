use api_runner::HostOverrides;
use async_trait::async_trait;
use plugin_runtime::*;
use std::{collections::BTreeMap, net::IpAddr};
fn addresses(config: &PluginConfig) -> Result<HostOverrides, PluginError> {
    let addresses = serde_json::from_value::<BTreeMap<String, Vec<IpAddr>>>(
        config
            .values
            .get("addresses")
            .cloned()
            .ok_or_else(|| PluginError::InvalidConfig("HOST addresses are required".into()))?,
    )
    .map_err(|_| {
        PluginError::InvalidConfig("HOST addresses must map DNS names to IP arrays".into())
    })?;
    if addresses.is_empty() {
        return Err(PluginError::InvalidConfig("provide at least one HOST override".into()));
    }
    HostOverrides::new(addresses).map_err(|e| PluginError::InvalidConfig(e.to_string()))
}
pub(super) struct HostProvider;
#[async_trait]
impl EnvironmentPlugin for HostProvider {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "host".into(),
            name: "HOST".into(),
            section: "host".into(),
            version: "1.0.0".into(),
            api_version: PLUGIN_API_VERSION,
            capabilities: vec![Capability::HostOverride, Capability::Execute],
            operations: vec!["execute".into()],
            config_schema: ConfigSchema {
                version: 1,
                fields: vec![ConfigField {
                    name: "addresses".into(),
                    label: "DNS names to IP arrays (JSON)".into(),
                    required: true,
                    kind: FieldType::Json,
                }],
            },
        }
    }
    fn validate_config(&self, config: &PluginConfig) -> Result<(), PluginError> {
        addresses(config).map(|_| ())
    }
    async fn execute(
        &self,
        invocation: &Invocation,
        _context: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError> {
        if !invocation.input.is_null() && invocation.input != serde_json::json!({}) {
            return Err(PluginError::InvalidConfig(
                "HOST resolution takes no caller overrides".into(),
            ));
        }
        Ok(PluginOutput::success(
            serde_json::json!({"addresses":addresses(&invocation.config)?.addresses()}),
        ))
    }
}
