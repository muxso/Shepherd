use async_trait::async_trait;
use plugin_runtime::*;
use probe::{
    domain::ssh::{SshAction, SshAuthentication, SshCommand, SshTarget},
    plugins::{
        ssh_command::{validate_target, SshBudget, SshError},
        SshPlugin,
    },
};
use serde_json::Value;
pub(super) struct SshProvider {
    driver: SshPlugin,
}
impl SshProvider {
    pub fn new() -> Self {
        Self { driver: SshPlugin::new() }
    }
}
fn field(name: &str, label: &str, required: bool, kind: FieldType) -> ConfigField {
    ConfigField { name: name.into(), label: label.into(), required, kind }
}
fn text(config: &PluginConfig, name: &str) -> Result<String, PluginError> {
    config
        .values
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| PluginError::InvalidConfig(format!("missing SSH field: {name}")))
}
fn target(config: &PluginConfig) -> Result<SshTarget, PluginError> {
    let port = config.values.get("port").and_then(Value::as_u64).unwrap_or(22);
    let port = u16::try_from(port)
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| PluginError::InvalidConfig("invalid SSH port".into()))?;
    Ok(SshTarget {
        host: text(config, "host")?,
        port,
        username: text(config, "username")?,
        host_key_sha256: text(config, "host_key_sha256")?,
    })
}
#[derive(Clone, Copy)]
enum Authentication {
    Password,
    PrivateKey,
}
fn authentication(config: &PluginConfig) -> Result<Authentication, PluginError> {
    let mode = config.values.get("authentication").and_then(Value::as_str).unwrap_or("private_key");
    let has = |field: &str| config.secrets.contains_key(field);
    match mode {
        "password" if has("password") && !has("private_key") && !has("passphrase")=>Ok(Authentication::Password),
        "private_key" if has("private_key") && !has("password")=>Ok(Authentication::PrivateKey),
        _=>Err(PluginError::InvalidConfig("SSH authentication requires matching credential references; password and key modes cannot be mixed".into())),
    }
}
fn error(error: SshError) -> PluginError {
    match error {
        SshError::InvalidTarget
        | SshError::InvalidFingerprint
        | SshError::PrivateKey
        | SshError::KeyCost
        | SshError::InvalidCommand
        | SshError::InvalidBudget => PluginError::InvalidConfig(error.to_string()),
        SshError::Timeout => PluginError::Timeout,
        SshError::OutputLimit => PluginError::OutputLimit,
        error => PluginError::Execution(error.to_string()),
    }
}
#[async_trait]
impl EnvironmentPlugin for SshProvider {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "ssh".into(),
            name: "SSH".into(),
            section: "ssh".into(),
            version: "1.0.0".into(),
            api_version: PLUGIN_API_VERSION,
            capabilities: vec![Capability::ConnectionTest, Capability::Execute],
            operations: vec!["test".into(), "execute".into()],
            config_schema: ConfigSchema {
                version: 1,
                fields: vec![
                    field("host", "Host", true, FieldType::String { max_length: 253 }),
                    field("port", "Port", false, FieldType::Integer { min: 1, max: 65535 }),
                    field("username", "Username", true, FieldType::String { max_length: 128 }),
                    field(
                        "host_key_sha256",
                        "Host key SHA256",
                        true,
                        FieldType::String { max_length: 128 },
                    ),
                    field(
                        "authentication",
                        "Authentication",
                        false,
                        FieldType::Choice {
                            options: vec!["private_key".into(), "password".into()],
                        },
                    ),
                    field("password", "Password", false, FieldType::Secret),
                    field("private_key", "Private key", false, FieldType::SecretMultiline),
                    field("passphrase", "Key passphrase", false, FieldType::Secret),
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
        validate_target(&target(config)?).map_err(error)?;
        authentication(config).map(|_| ())
    }
    async fn execute(
        &self,
        invocation: &Invocation,
        context: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError> {
        let target = target(&invocation.config)?;
        let command = if invocation.operation == "test" {
            None
        } else {
            Some(
                serde_json::from_value::<SshCommand>(invocation.input.clone())
                    .map_err(|_| PluginError::InvalidConfig("invalid SSH command input".into()))?,
            )
        };
        let mode = authentication(&invocation.config)?;
        let field = match mode {
            Authentication::Password => "password",
            Authentication::PrivateKey => "private_key",
        };
        let reference =
            invocation.config.secrets.get(field).ok_or(PluginError::PermissionDenied)?;
        let credential = context.secrets.resolve(&context.scope, reference).await?;
        let passphrase = match invocation.config.secrets.get("passphrase") {
            Some(reference) => Some(context.secrets.resolve(&context.scope, reference).await?),
            None => None,
        };
        let authentication = match mode {
            Authentication::Password => SshAuthentication::Password(credential.expose()),
            Authentication::PrivateKey => SshAuthentication::PrivateKey {
                key: credential.expose(),
                passphrase: passphrase.as_ref().map(SecretValue::expose),
            },
        };
        let action = match &command {
            None => SshAction::Test,
            Some(command) => SshAction::Execute(command),
        };
        let timeout = invocation
            .config
            .values
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(context.limits.timeout_ms)
            .min(context.limits.timeout_ms);
        let report = self
            .driver
            .execute_command(
                &target,
                authentication,
                action,
                SshBudget { timeout_ms: timeout, max_bytes: context.limits.max_output_bytes },
            )
            .await
            .map_err(error)?;
        let passed = report.success();
        let mut output = PluginOutput::success(
            serde_json::to_value(report)
                .map_err(|_| PluginError::Execution("invalid SSH report".into()))?,
        );
        if command.is_some() {
            output.assertions.push(AssertionResult {
                id: "ssh.exit".into(),
                passed,
                message: if passed {
                    "command exited successfully"
                } else {
                    "command failed, was signalled, or did not report an exit status"
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
