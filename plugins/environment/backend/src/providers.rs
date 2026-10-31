//! Profiles adapt existing protocol engines; credentials are resolved only during execution.
use async_trait::async_trait;
use plugin_runtime::*;
use probe::{ProbeRequest, ProtocolPlugin};
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeMap, net::IpAddr, sync::Arc};

pub fn registry() -> Result<PluginRegistry, PluginError> {
    let mut registry = PluginRegistry::default();
    registry.register(Arc::new(DatabaseProvider {
        id: "postgres",
        protocol: "sql",
        driver: Arc::new(probe::plugins::SqlPlugin::new()),
    }))?;
    registry.register(Arc::new(DatabaseProvider {
        id: "mysql",
        protocol: "mysql",
        driver: Arc::new(probe::plugins::MysqlPlugin::new()),
    }))?;
    Ok(registry)
}
struct DatabaseProvider {
    id: &'static str,
    protocol: &'static str,
    driver: Arc<dyn ProtocolPlugin>,
}
fn field(name: &str, label: &str, required: bool, kind: FieldType) -> ConfigField {
    ConfigField { name: name.into(), label: label.into(), required, kind }
}
fn string(config: &PluginConfig, name: &str) -> Result<String, PluginError> {
    config
        .values
        .get(name)
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| PluginError::InvalidConfig(format!("missing field: {name}")))
}
impl DatabaseProvider {
    fn target(&self, config: &PluginConfig, password: Option<&str>) -> Result<String, PluginError> {
        let host = string(config, "host")?;
        let host = match host.parse::<IpAddr>() {
            Ok(IpAddr::V6(_)) => format!("[{host}]"),
            _ => host,
        };
        let mut target = url::Url::parse(&format!(
            "{}://placeholder",
            if self.protocol == "sql" { "postgres" } else { "mysql" }
        ))
        .map_err(|_| PluginError::InvalidConfig("invalid database scheme".into()))?;
        target
            .set_host(Some(&host))
            .map_err(|_| PluginError::InvalidConfig("invalid database host".into()))?;
        target
            .set_port(Some(
                config
                    .values
                    .get("port")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(if self.protocol == "sql" { 5432 } else { 3306 })
                    as u16,
            ))
            .map_err(|_| PluginError::InvalidConfig("invalid database port".into()))?;
        target
            .set_username(&string(config, "user")?)
            .map_err(|_| PluginError::InvalidConfig("invalid database user".into()))?;
        target
            .set_password(password)
            .map_err(|_| PluginError::InvalidConfig("invalid database credential".into()))?;
        target
            .path_segments_mut()
            .map_err(|_| PluginError::InvalidConfig("invalid database name".into()))?
            .clear()
            .push(&string(config, "database")?);
        let tls = config.values.get("tls").and_then(|v| v.as_str()).unwrap_or("require");
        target.query_pairs_mut().append_pair(
            if self.protocol == "sql" { "sslmode" } else { "ssl-mode" },
            if self.protocol != "sql" && tls == "disable" {
                "DISABLED"
            } else if self.protocol != "sql" {
                "REQUIRED"
            } else {
                tls
            },
        );
        Ok(target.into())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryInput {
    query: String,
}

#[async_trait]
impl EnvironmentPlugin for DatabaseProvider {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: self.id.into(),
            name: if self.protocol == "sql" { "PostgreSQL" } else { "MySQL" }.into(),
            section: "database".into(),
            version: "1.0.0".into(),
            api_version: PLUGIN_API_VERSION,
            capabilities: vec![Capability::ConnectionTest, Capability::Execute],
            operations: vec!["test".into(), "execute".into()],
            config_schema: ConfigSchema {
                version: 1,
                fields: vec![
                    field("host", "Host", true, FieldType::String { max_length: 253 }),
                    field("port", "Port", false, FieldType::Integer { min: 1, max: 65535 }),
                    field("database", "Database", true, FieldType::String { max_length: 128 }),
                    field("user", "User", true, FieldType::String { max_length: 128 }),
                    field("password", "Password", false, FieldType::Secret),
                    field(
                        "tls",
                        "TLS",
                        false,
                        FieldType::Choice { options: vec!["require".into(), "disable".into()] },
                    ),
                ],
            },
        }
    }
    fn validate_config(&self, config: &PluginConfig) -> Result<(), PluginError> {
        self.target(config, None).map(|_| ())
    }
    async fn execute(
        &self,
        invocation: &Invocation,
        context: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError> {
        let password = match invocation.config.secrets.get("password") {
            Some(reference) => Some(context.secrets.resolve(&context.scope, reference).await?),
            None => None,
        };
        let target = self.target(&invocation.config, password.as_ref().map(SecretValue::expose))?;
        let query = if invocation.operation == "test" {
            "SELECT 1".to_string()
        } else {
            let query = serde_json::from_value::<QueryInput>(invocation.input.clone())
                .map_err(|_| PluginError::InvalidConfig("expected a query string".into()))?
                .query;
            if query.trim().is_empty() || query.len() > 65_536 {
                return Err(PluginError::InvalidConfig("query is empty or too large".into()));
            }
            query
        };
        let result = self
            .driver
            .run(&ProbeRequest {
                protocol: self.protocol.into(),
                target,
                payload: Some(query),
                metadata: BTreeMap::new(),
                assertions: Vec::new(),
            })
            .await;
        if !result.transport_ok {
            return Err(PluginError::Execution(
                result.error.unwrap_or_else(|| "database operation failed".into()),
            ));
        }
        Ok(PluginOutput::success(
            json!({"status":result.status,"latencyMs":result.latency_ms,"output":result.output}),
        ))
    }
}
