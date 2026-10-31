use crate::preparation::Step;
use async_trait::async_trait;
use plugin_runtime::*;
pub(super) struct SetupProvider;
pub(crate) fn steps(config: &PluginConfig) -> Result<Vec<Step>, PluginError> {
    let steps: Vec<Step> = serde_json::from_value(
        config
            .values
            .get("steps")
            .cloned()
            .ok_or_else(|| PluginError::InvalidConfig("setup steps are required".into()))?,
    )
    .map_err(|_| PluginError::InvalidConfig("invalid setup steps".into()))?;
    if steps.is_empty() || steps.len() > 32 {
        return Err(PluginError::InvalidConfig("provide 1–32 setup steps".into()));
    }
    for step in &steps {
        step.validate()?;
    }
    Ok(steps)
}
#[async_trait]
impl EnvironmentPlugin for SetupProvider {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "setup".into(),
            name: "Setup pipeline".into(),
            section: "pre".into(),
            version: "1.0.0".into(),
            api_version: PLUGIN_API_VERSION,
            capabilities: vec![Capability::Preprocess],
            operations: vec!["plan".into()],
            config_schema: ConfigSchema {
                version: 1,
                fields: vec![ConfigField {
                    name: "steps".into(),
                    label: "Ordered setup steps (JSON)".into(),
                    required: true,
                    kind: FieldType::Json,
                }],
            },
        }
    }
    fn validate_config(&self, config: &PluginConfig) -> Result<(), PluginError> {
        steps(config).map(|_| ())
    }
    async fn execute(
        &self,
        invocation: &Invocation,
        _: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError> {
        Ok(PluginOutput::success(serde_json::json!({"steps":steps(&invocation.config)?})))
    }
}
