use async_trait::async_trait;
use plugin_runtime::*;
use probe::{
    domain::redis::{RedisArgument, RedisCommand},
    plugins::{
        redis_command::{RedisBudget, RedisCommandError},
        RedisPlugin,
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    net::IpAddr,
};

pub(super) struct RedisProvider {
    driver: RedisPlugin,
}
impl RedisProvider {
    pub fn new() -> Self {
        Self { driver: RedisPlugin::new() }
    }
}
fn field(name: &str, label: &str, required: bool, kind: FieldType) -> ConfigField {
    ConfigField { name: name.into(), label: label.into(), required, kind }
}
fn target(config: &PluginConfig, password: Option<&str>) -> Result<String, PluginError> {
    let host = config
        .values
        .get("host")
        .and_then(Value::as_str)
        .filter(|host| !host.trim().is_empty())
        .ok_or_else(|| PluginError::InvalidConfig("missing Redis host".into()))?;
    let tls = config.values.get("tls").and_then(Value::as_str).unwrap_or("require");
    let scheme = match tls {
        "require" => "rediss",
        "disable" => "redis",
        _ => return Err(PluginError::InvalidConfig("invalid TLS setting".into())),
    };
    let mut url = url::Url::parse(&format!("{scheme}://placeholder"))
        .map_err(|_| PluginError::InvalidConfig("invalid Redis scheme".into()))?;
    let host = match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(_)) => format!("[{host}]"),
        _ => host.into(),
    };
    url.set_host(Some(&host))
        .map_err(|_| PluginError::InvalidConfig("invalid Redis host".into()))?;
    let port = config.values.get("port").and_then(Value::as_u64).unwrap_or(6379);
    let port = u16::try_from(port)
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| PluginError::InvalidConfig("invalid Redis port".into()))?;
    url.set_port(Some(port))
        .map_err(|_| PluginError::InvalidConfig("invalid Redis port".into()))?;
    if let Some(username) = config.values.get("username").and_then(Value::as_str) {
        if username.trim().is_empty() || !config.secrets.contains_key("password") {
            return Err(PluginError::InvalidConfig(
                "Redis username requires a credential reference".into(),
            ));
        }
        url.set_username(username)
            .map_err(|_| PluginError::InvalidConfig("invalid Redis username".into()))?;
    }
    url.set_password(password)
        .map_err(|_| PluginError::InvalidConfig("invalid Redis credential".into()))?;
    let database = config.values.get("database").and_then(Value::as_u64).unwrap_or(0);
    if database > i32::MAX as u64 {
        return Err(PluginError::InvalidConfig("invalid Redis database".into()));
    }
    url.set_path(&format!("/{database}"));
    let protocol = config.values.get("protocol").and_then(Value::as_str).unwrap_or("resp2");
    if !matches!(protocol, "resp2" | "resp3") {
        return Err(PluginError::InvalidConfig("invalid Redis protocol".into()));
    }
    url.query_pairs_mut().append_pair("protocol", protocol);
    Ok(url.into())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    command: String,
    #[serde(default)]
    arguments: Vec<RedisArgument>,
    #[serde(default)]
    extract: Vec<Extraction>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Extraction {
    variable: String,
    pointer: String,
    #[serde(default = "required")]
    required: bool,
}
fn required() -> bool {
    true
}
fn validate_extract(rules: &[Extraction]) -> Result<(), PluginError> {
    let mut names = BTreeSet::new();
    if rules.len() > 64
        || rules.iter().any(|rule| {
            rule.variable.is_empty()
                || rule.variable.len() > 64
                || !rule.variable.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                || !names.insert(&rule.variable)
                || rule.pointer.len() > 1024
                || (!rule.pointer.is_empty() && !rule.pointer.starts_with('/'))
                || rule
                    .pointer
                    .as_bytes()
                    .windows(2)
                    .any(|pair| pair[0] == b'~' && !matches!(pair[1], b'0' | b'1'))
                || rule.pointer.ends_with('~')
        })
    {
        return Err(PluginError::InvalidConfig(
            "invalid or duplicate Redis extraction rule".into(),
        ));
    }
    Ok(())
}
fn extract(
    value: &Value,
    rules: &[Extraction],
) -> (BTreeMap<String, String>, Vec<AssertionResult>) {
    let mut variables = BTreeMap::new();
    let mut assertions = Vec::new();
    for rule in rules {
        let cell = value.pointer(&rule.pointer);
        if let Some(value) = cell {
            variables.insert(
                rule.variable.clone(),
                match value {
                    Value::String(value) => value.clone(),
                    value => value.to_string(),
                },
            );
        }
        if rule.required {
            assertions.push(AssertionResult {
                id: format!("extract.{}", rule.variable),
                passed: cell.is_some(),
                message: if cell.is_some() {
                    "value extracted"
                } else {
                    "required Redis result path is missing"
                }
                .into(),
            });
        }
    }
    (variables, assertions)
}
fn execution_error(error: RedisCommandError) -> PluginError {
    match error {
        RedisCommandError::InvalidRequest(error) => PluginError::InvalidConfig(error.to_string()),
        RedisCommandError::InvalidBudget => {
            PluginError::InvalidConfig("invalid Redis execution budget".into())
        }
        RedisCommandError::Timeout => PluginError::Timeout,
        RedisCommandError::OutputLimit => PluginError::OutputLimit,
        error => PluginError::Execution(error.to_string()),
    }
}
#[async_trait]
impl EnvironmentPlugin for RedisProvider {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "redis".into(),
            name: "Redis".into(),
            section: "redis".into(),
            version: "1.0.0".into(),
            api_version: PLUGIN_API_VERSION,
            capabilities: vec![Capability::ConnectionTest, Capability::Execute],
            operations: vec!["test".into(), "execute".into()],
            config_schema: ConfigSchema {
                version: 1,
                fields: vec![
                    field("host", "Host", true, FieldType::String { max_length: 253 }),
                    field("port", "Port", false, FieldType::Integer { min: 1, max: 65535 }),
                    field(
                        "database",
                        "Database",
                        false,
                        FieldType::Integer { min: 0, max: i32::MAX as i64 },
                    ),
                    field("username", "Username", false, FieldType::String { max_length: 128 }),
                    field("password", "Password", false, FieldType::Secret),
                    field(
                        "tls",
                        "TLS",
                        false,
                        FieldType::Choice { options: vec!["require".into(), "disable".into()] },
                    ),
                    field(
                        "protocol",
                        "Protocol",
                        false,
                        FieldType::Choice { options: vec!["resp2".into(), "resp3".into()] },
                    ),
                ],
            },
        }
    }
    fn validate_config(&self, config: &PluginConfig) -> Result<(), PluginError> {
        target(config, None).map(|_| ())
    }
    async fn execute(
        &self,
        invocation: &Invocation,
        context: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError> {
        let (command, rules) = if invocation.operation == "test" {
            (RedisCommand { command: "PING".into(), arguments: Vec::new() }, Vec::new())
        } else {
            let input: Input = serde_json::from_value(invocation.input.clone())
                .map_err(|_| PluginError::InvalidConfig("invalid Redis command input".into()))?;
            validate_extract(&input.extract)?;
            (RedisCommand { command: input.command, arguments: input.arguments }, input.extract)
        };
        command.validate().map_err(|error| PluginError::InvalidConfig(error.to_string()))?;
        let password = match invocation.config.secrets.get("password") {
            Some(reference) => Some(context.secrets.resolve(&context.scope, reference).await?),
            None => None,
        };
        let target = target(&invocation.config, password.as_ref().map(SecretValue::expose))?;
        let report = self
            .driver
            .execute_command(
                &target,
                &command,
                RedisBudget {
                    max_bytes: context.limits.max_output_bytes,
                    timeout_ms: context.limits.timeout_ms,
                },
            )
            .await
            .map_err(execution_error)?;
        let (variables, assertions) = extract(&report.value, &rules);
        let status = if assertions.iter().all(|assertion| assertion.passed) {
            OutputStatus::Success
        } else {
            OutputStatus::Failure
        };
        Ok(PluginOutput {
            status,
            data: serde_json::to_value(report)
                .map_err(|_| PluginError::Execution("invalid Redis report".into()))?,
            variables,
            assertions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn extraction_distinguishes_missing_paths_and_present_nulls() {
        let rules:Vec<Extraction>=serde_json::from_value(json!([
            {"variable":"null","pointer":"/0"}, {"variable":"value","pointer":"/1"},
            {"variable":"missing","pointer":"/2"}, {"variable":"optional","pointer":"/3","required":false}
        ])).expect("rules");
        validate_extract(&rules).expect("valid rules");
        let (variables, assertions) = extract(&json!([null, "hello world"]), &rules);
        assert_eq!(variables["null"], "null");
        assert_eq!(variables["value"], "hello world");
        assert!(!variables.contains_key("optional"));
        assert_eq!(assertions.iter().filter(|assertion| !assertion.passed).count(), 1);
        let invalid: Vec<Extraction> =
            serde_json::from_value(json!([{"variable":"value","pointer":"/~x"}])).expect("rule");
        assert!(validate_extract(&invalid).is_err());
    }
    #[test]
    fn profile_urls_encode_credentials_and_keep_verified_tls_and_protocol() {
        let config = PluginConfig {
            schema_version: 1,
            values: [
                ("host".into(), json!("::1")),
                ("database".into(), json!(13)),
                ("username".into(), json!("user@name")),
                ("protocol".into(), json!("resp3")),
            ]
            .into(),
            secrets: [("password".into(), SecretRef("credential".into()))].into(),
        };
        let target = target(&config, Some("password @:/?#")).expect("target");
        let url = url::Url::parse(&target).expect("URL");
        assert_eq!(url.scheme(), "rediss");
        assert_eq!(url.path(), "/13");
        assert_eq!(url.username(), "user%40name");
        assert!(url.password().expect("password").contains("%40"));
        assert!(url.fragment().is_none());
        assert_eq!(url.query_pairs().next().expect("protocol").1, "resp3");
    }
}
