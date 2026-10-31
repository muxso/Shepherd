use async_trait::async_trait;
use plugin_runtime::*;
use probe::{
    domain::grpc::GrpcCall,
    plugins::{
        grpc_dynamic::{
            validate_endpoint, validate_metadata, CompiledSchema, GrpcAction, GrpcBudget,
            GrpcCredential, GrpcError, GrpcIdentity, GrpcTarget,
        },
        GrpcPlugin,
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
pub(super) struct GrpcProvider {
    driver: GrpcPlugin,
}
impl GrpcProvider {
    pub fn new() -> Self {
        Self { driver: GrpcPlugin::new() }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    endpoint: String,
    proto_files: BTreeMap<String, String>,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
    server_name: Option<String>,
    credential_name: Option<String>,
    timeout_ms: Option<u64>,
}
impl Settings {
    fn load(config: &PluginConfig) -> Result<Self, PluginError> {
        serde_json::from_value(
            serde_json::to_value(&config.values)
                .map_err(|_| PluginError::InvalidConfig("invalid gRPC settings".into()))?,
        )
        .map_err(|_| PluginError::InvalidConfig("invalid gRPC settings".into()))
    }
    fn credential_name<'a>(&'a self, config: &PluginConfig) -> Option<&'a str> {
        config
            .secrets
            .contains_key("credential")
            .then(|| self.credential_name.as_deref().unwrap_or("authorization"))
    }
    fn validate(&self, config: &PluginConfig) -> Result<(), PluginError> {
        let has = |name: &str| config.secrets.contains_key(name);
        if has("client_certificate") != has("client_key")
            || (self.credential_name.is_some() && !has("credential"))
            || self.server_name.as_ref().is_some_and(|name| name.trim().is_empty())
        {
            return Err(PluginError::InvalidConfig("gRPC TLS identity requires both certificate/key references; credential names require a credential reference".into()));
        }
        validate_endpoint(
            &self.endpoint,
            has("ca_certificate") || has("client_certificate") || self.server_name.is_some(),
        )
        .map_err(error)?;
        validate_metadata(&self.metadata, self.credential_name(config)).map_err(error)
    }
    fn schema(&self) -> Result<CompiledSchema, PluginError> {
        CompiledSchema::compile(&self.proto_files)
            .map_err(|error| PluginError::InvalidConfig(error.to_string()))
    }
}
fn error(error: GrpcError) -> PluginError {
    match error {
        GrpcError::Schema(_)
        | GrpcError::InvalidTarget
        | GrpcError::InvalidMetadata
        | GrpcError::InvalidMessage
        | GrpcError::InvalidBudget => PluginError::InvalidConfig(error.to_string()),
        GrpcError::Timeout => PluginError::Timeout,
        GrpcError::OutputLimit => PluginError::OutputLimit,
        error => PluginError::Execution(error.to_string()),
    }
}
fn field(name: &str, label: &str, required: bool, kind: FieldType) -> ConfigField {
    ConfigField { name: name.into(), label: label.into(), required, kind }
}
async fn secret(
    config: &PluginConfig,
    name: &str,
    context: &ExecutionContext,
) -> Result<Option<SecretValue>, PluginError> {
    match config.secrets.get(name) {
        None => Ok(None),
        Some(reference) => context.secrets.resolve(&context.scope, reference).await.map(Some),
    }
}
#[async_trait]
impl EnvironmentPlugin for GrpcProvider {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "grpc".into(),
            name: "gRPC".into(),
            section: "grpc".into(),
            version: "1.0.0".into(),
            api_version: PLUGIN_API_VERSION,
            capabilities: vec![Capability::ConnectionTest, Capability::Execute],
            operations: vec!["test".into(), "execute".into()],
            config_schema: ConfigSchema {
                version: 1,
                fields: vec![
                    field("endpoint", "Endpoint", true, FieldType::String { max_length: 2048 }),
                    field("proto_files", "Proto files (JSON)", true, FieldType::Json),
                    field("metadata", "Public metadata (JSON)", false, FieldType::Json),
                    field("credential", "Metadata credential", false, FieldType::Secret),
                    field(
                        "credential_name",
                        "Credential metadata name",
                        false,
                        FieldType::String { max_length: 128 },
                    ),
                    field(
                        "server_name",
                        "TLS server name",
                        false,
                        FieldType::String { max_length: 253 },
                    ),
                    field("ca_certificate", "CA certificate", false, FieldType::SecretMultiline),
                    field(
                        "client_certificate",
                        "Client certificate",
                        false,
                        FieldType::SecretMultiline,
                    ),
                    field("client_key", "Client private key", false, FieldType::SecretMultiline),
                    field(
                        "timeout_ms",
                        "Timeout (ms)",
                        false,
                        FieldType::Integer { min: 1, max: 600_000 },
                    ),
                ],
            },
        }
    }
    fn validate_config(&self, config: &PluginConfig) -> Result<(), PluginError> {
        let settings = Settings::load(config)?;
        settings.validate(config)?;
        settings.schema().map(|_| ())
    }
    fn inspect_config(&self, config: &PluginConfig) -> Result<Value, PluginError> {
        let settings = Settings::load(config)?;
        settings.validate(config)?;
        Ok(serde_json::json!({"methods":settings.schema()?.methods()}))
    }
    async fn execute(
        &self,
        invocation: &Invocation,
        context: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError> {
        let settings = Settings::load(&invocation.config)?;
        settings.validate(&invocation.config)?;
        let schema = settings.schema()?;
        let call = if invocation.operation == "test" {
            None
        } else {
            Some(
                serde_json::from_value::<GrpcCall>(invocation.input.clone())
                    .map_err(|_| PluginError::InvalidConfig("invalid gRPC call input".into()))?,
            )
        };
        let credential = secret(&invocation.config, "credential", context).await?;
        let ca = secret(&invocation.config, "ca_certificate", context).await?;
        let certificate = secret(&invocation.config, "client_certificate", context).await?;
        let key = secret(&invocation.config, "client_key", context).await?;
        let identity = match (&certificate, &key) {
            (None, None) => None,
            (Some(certificate), Some(key)) => {
                Some(GrpcIdentity { certificate: certificate.expose(), private_key: key.expose() })
            }
            _ => return Err(PluginError::InvalidConfig("incomplete gRPC TLS identity".into())),
        };
        let credential = credential.as_ref().map(|value| GrpcCredential {
            name: settings.credential_name(&invocation.config).unwrap_or("authorization"),
            value: value.expose(),
        });
        let target = GrpcTarget {
            endpoint: &settings.endpoint,
            server_name: settings.server_name.as_deref(),
            ca_certificate: ca.as_ref().map(SecretValue::expose),
            identity,
            metadata: &settings.metadata,
            credential,
        };
        let action = match &call {
            None => GrpcAction::Test,
            Some(call) => GrpcAction::Call(call),
        };
        let report = self
            .driver
            .execute_call(
                &schema,
                &target,
                action,
                GrpcBudget {
                    timeout_ms: settings
                        .timeout_ms
                        .unwrap_or(context.limits.timeout_ms)
                        .min(context.limits.timeout_ms),
                    max_bytes: context.limits.max_output_bytes,
                },
            )
            .await
            .map_err(error)?;
        let passed = report.success();
        let mut output = PluginOutput::success(
            serde_json::to_value(report)
                .map_err(|_| PluginError::Execution("invalid gRPC report".into()))?,
        );
        if call.is_some() {
            output.assertions.push(AssertionResult {
                id: "grpc.status".into(),
                passed,
                message: if passed {
                    "gRPC status OK"
                } else {
                    "gRPC method returned a non-OK status"
                }
                .into(),
            });
        }
        if !passed {
            output.status = OutputStatus::Failure;
        }
        Ok(output)
    }
}
