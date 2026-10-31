use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const PLUGIN_API_VERSION: u32 = 1;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PluginError {
    #[error("invalid plugin contract: {0}")]
    InvalidContract(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("plugin not installed or disabled: {0}")]
    Unavailable(String),
    #[error("unsupported operation: {0}")]
    UnsupportedOperation(String),
    #[error("permission denied")]
    PermissionDenied,
    #[error("execution cancelled")]
    Cancelled,
    #[error("execution timed out")]
    Timeout,
    #[error("plugin output exceeds execution limit")]
    OutputLimit,
    #[error("plugin execution failed: {0}")]
    Execution(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ConnectionTest,
    Execute,
    Preprocess,
    Postprocess,
    Assert,
    Display,
    HostOverride,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FieldType {
    String { max_length: usize },
    Multiline { max_length: usize },
    Integer { min: i64, max: i64 },
    Boolean,
    Choice { options: Vec<String> },
    StringList { max_items: usize },
    Json,
    Secret,
    SecretMultiline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigField {
    pub name: String,
    pub label: String,
    pub required: bool,
    pub kind: FieldType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigSchema {
    pub version: u32,
    pub fields: Vec<ConfigField>,
}

/// Opaque reference only. The resolver authorizes it against the execution scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretRef(pub String);

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub secrets: BTreeMap<String, SecretRef>,
}

// Even invalid, not-yet-validated input must not leak plaintext through debug logging.
impl fmt::Debug for PluginConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginConfig")
            .field("schema_version", &self.schema_version)
            .field("configured_fields", &self.values.keys().collect::<Vec<_>>())
            .field("secret_fields", &self.secrets.keys().collect::<Vec<_>>())
            .finish()
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_lowercase()
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
}

impl ConfigSchema {
    pub fn validate_definition(&self) -> Result<(), PluginError> {
        let mut names = BTreeSet::new();
        if self.version == 0 {
            return Err(PluginError::InvalidContract("schema version must be positive".into()));
        }
        for field in &self.fields {
            let valid = match &field.kind {
                FieldType::String { max_length } | FieldType::Multiline { max_length } => {
                    *max_length > 0
                }
                FieldType::Integer { min, max } => min <= max,
                FieldType::Choice { options } => {
                    !options.is_empty()
                        && options.iter().all(|s| !s.is_empty())
                        && options.iter().collect::<BTreeSet<_>>().len() == options.len()
                }
                FieldType::StringList { max_items } => *max_items > 0,
                _ => true,
            };
            if !valid_name(&field.name)
                || field.label.trim().is_empty()
                || !valid
                || !names.insert(&field.name)
            {
                return Err(PluginError::InvalidContract(
                    "invalid or duplicate configuration field".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn validate(&self, config: &PluginConfig) -> Result<(), PluginError> {
        self.validate_definition()?;
        if config.schema_version != self.version {
            return Err(PluginError::InvalidConfig(
                "schema version mismatch; migrate the profile first".into(),
            ));
        }
        for name in config.values.keys().chain(config.secrets.keys()) {
            if !self.fields.iter().any(|f| &f.name == name) {
                return Err(PluginError::InvalidConfig("unknown configuration field".into()));
            }
        }
        for field in &self.fields {
            let value = config.values.get(&field.name);
            let secret = config.secrets.get(&field.name);
            let valid = match (&field.kind, value, secret) {
                (FieldType::Secret | FieldType::SecretMultiline, None, Some(reference)) => {
                    !reference.0.trim().is_empty() && reference.0.len() <= 256
                }
                (_, None, None) => !field.required,
                (_, _, Some(_)) | (FieldType::Secret | FieldType::SecretMultiline, Some(_), _) => {
                    false
                }
                (
                    FieldType::String { max_length } | FieldType::Multiline { max_length },
                    Some(v),
                    _,
                ) => v.as_str().is_some_and(|s| s.len() <= *max_length),
                (FieldType::Integer { min, max }, Some(v), _) => {
                    v.as_i64().is_some_and(|n| (*min..=*max).contains(&n))
                }
                (FieldType::Boolean, Some(v), _) => v.is_boolean(),
                (FieldType::Choice { options }, Some(v), _) => {
                    v.as_str().is_some_and(|s| options.iter().any(|o| o == s))
                }
                (FieldType::StringList { max_items }, Some(v), _) => v
                    .as_array()
                    .is_some_and(|a| a.len() <= *max_items && a.iter().all(Value::is_string)),
                (FieldType::Json, Some(_), None) => true,
            };
            if !valid {
                // Never interpolate the supplied value into errors.
                return Err(PluginError::InvalidConfig(format!(
                    "missing or invalid field: {}",
                    field.name
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginDescriptor {
    pub id: String,
    pub name: String,
    pub section: String,
    pub version: String,
    pub api_version: u32,
    pub capabilities: Vec<Capability>,
    pub operations: Vec<String>,
    pub config_schema: ConfigSchema,
}

impl PluginDescriptor {
    pub fn validate(&self) -> Result<(), PluginError> {
        if !valid_name(&self.id)
            || self.name.trim().is_empty()
            || !valid_name(&self.section)
            || self.version.trim().is_empty()
            || self.api_version != PLUGIN_API_VERSION
            || self.capabilities.is_empty()
            || self.operations.is_empty()
            || self.operations.iter().any(|o| !valid_name(o))
            || self.operations.iter().collect::<BTreeSet<_>>().len() != self.operations.len()
        {
            return Err(PluginError::InvalidContract(
                "invalid identity, API version, capabilities, or operations".into(),
            ));
        }
        self.config_schema.validate_definition()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionScope {
    pub project_id: String,
    pub environment_id: String,
    pub actor_id: String,
}

impl ExecutionScope {
    pub fn validate(&self) -> Result<(), PluginError> {
        if [&self.project_id, &self.environment_id, &self.actor_id]
            .iter()
            .any(|v| v.trim().is_empty())
        {
            return Err(PluginError::PermissionDenied);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionLimits {
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self { timeout_ms: 30_000, max_output_bytes: 1_048_576 }
    }
}

impl ExecutionLimits {
    pub fn validate(&self) -> Result<(), PluginError> {
        if !(1..=600_000).contains(&self.timeout_ms)
            || !(1..=16_777_216).contains(&self.max_output_bytes)
        {
            return Err(PluginError::InvalidConfig("execution limits out of range".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Invocation {
    pub plugin_id: String,
    pub operation: String,
    pub config: PluginConfig,
    #[serde(default)]
    pub input: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStatus {
    Success,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssertionResult {
    pub id: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginOutput {
    pub status: OutputStatus,
    pub data: Value,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    pub assertions: Vec<AssertionResult>,
}

impl PluginOutput {
    pub fn success(data: Value) -> Self {
        Self {
            status: OutputStatus::Success,
            data,
            variables: BTreeMap::new(),
            assertions: Vec::new(),
        }
    }
    pub fn passed(&self) -> bool {
        self.status == OutputStatus::Success && self.assertions.iter().all(|a| a.passed)
    }
}
