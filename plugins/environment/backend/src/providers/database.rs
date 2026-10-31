//! Profiles adapt existing protocol engines; credentials are resolved only during execution.
use async_trait::async_trait;
use plugin_runtime::*;
use probe::{
    domain::database::{DatabaseReport, DatabaseStatement, SqlParameter, StatementMode},
    plugins::{
        database::{DatabaseError, QueryBudget},
        MysqlPlugin, SqlPlugin,
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, net::IpAddr, sync::Arc};

pub(super) fn registry() -> Result<PluginRegistry, PluginError> {
    let mut registry = PluginRegistry::default();
    registry.register(Arc::new(DatabaseProvider::Postgres(SqlPlugin::new())))?;
    registry.register(Arc::new(DatabaseProvider::MySql(MysqlPlugin::new())))?;
    Ok(registry)
}
enum DatabaseProvider {
    Postgres(SqlPlugin),
    MySql(MysqlPlugin),
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
    fn id(&self) -> &'static str {
        match self {
            Self::Postgres(_) => "postgres",
            Self::MySql(_) => "mysql",
        }
    }
    async fn query(
        &self,
        target: &str,
        statement: &DatabaseStatement,
        budget: QueryBudget,
    ) -> Result<DatabaseReport, DatabaseError> {
        match self {
            Self::Postgres(driver) => driver.query(target, statement, budget).await,
            Self::MySql(driver) => driver.query(target, statement, budget).await,
        }
    }

    fn target(&self, config: &PluginConfig, password: Option<&str>) -> Result<String, PluginError> {
        let host = string(config, "host")?;
        let host = match host.parse::<IpAddr>() {
            Ok(IpAddr::V6(_)) => format!("[{host}]"),
            _ => host,
        };
        let (scheme, default_port, tls_key, tls_required, tls_disabled) = match self {
            Self::Postgres(_) => ("postgres", 5432, "sslmode", "require", "disable"),
            Self::MySql(_) => ("mysql", 3306, "ssl-mode", "REQUIRED", "DISABLED"),
        };
        let mut target = url::Url::parse(&format!("{scheme}://placeholder"))
            .map_err(|_| PluginError::InvalidConfig("invalid database scheme".into()))?;
        target
            .set_host(Some(&host))
            .map_err(|_| PluginError::InvalidConfig("invalid database host".into()))?;
        let port = config.values.get("port").and_then(|v| v.as_u64()).unwrap_or(default_port);
        let port = u16::try_from(port)
            .ok()
            .filter(|p| *p != 0)
            .ok_or_else(|| PluginError::InvalidConfig("invalid database port".into()))?;
        target
            .set_port(Some(port))
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
        let tls = match config.values.get("tls").and_then(|v| v.as_str()) {
            None | Some("require") => tls_required,
            Some("disable") => tls_disabled,
            _ => return Err(PluginError::InvalidConfig("invalid TLS setting".into())),
        };
        target.query_pairs_mut().append_pair(tls_key, tls);
        Ok(target.into())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QueryInput {
    query: String,
    #[serde(default)]
    mode: StatementMode,
    #[serde(default)]
    parameters: Vec<SqlParameter>,
    #[serde(default = "default_max_rows")]
    max_rows: usize,
    #[serde(default)]
    extract: Vec<Extraction>,
}
fn default_max_rows() -> usize {
    100
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Extraction {
    variable: String,
    row: usize,
    column: ColumnSelector,
    #[serde(default = "required")]
    required: bool,
}
fn required() -> bool {
    true
}
#[derive(Deserialize)]
#[serde(untagged)]
enum ColumnSelector {
    Index(usize),
    Name(String),
}
impl QueryInput {
    fn validate(&self) -> Result<(), PluginError> {
        if self.mode == StatementMode::Execute && !self.extract.is_empty() {
            return Err(PluginError::InvalidConfig("result extraction requires query mode".into()));
        }
        let mut names = std::collections::BTreeSet::new();
        if self.extract.len() > 64
            || self.extract.iter().any(|e| {
                e.variable.is_empty()
                    || e.variable.len() > 64
                    || !e.variable.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    || !names.insert(&e.variable)
            })
        {
            return Err(PluginError::InvalidConfig(
                "invalid or duplicate extraction variables".into(),
            ));
        }
        Ok(())
    }
}
fn database_error(error: DatabaseError) -> PluginError {
    match error {
        DatabaseError::InvalidRequest(message) => PluginError::InvalidConfig(message.to_string()),
        DatabaseError::InvalidBudget => PluginError::InvalidConfig("invalid query budget".into()),
        DatabaseError::OutputLimit => PluginError::OutputLimit,
        DatabaseError::Timeout => PluginError::Timeout,
        error => PluginError::Execution(error.to_string()),
    }
}
fn output(report: DatabaseReport, extract: &[Extraction]) -> Result<PluginOutput, PluginError> {
    let mut variables = BTreeMap::new();
    let mut assertions = Vec::new();
    for extraction in extract {
        let index = match &extraction.column {
            ColumnSelector::Index(index) => Some(*index),
            ColumnSelector::Name(name) => {
                let mut matches =
                    report.columns.iter().enumerate().filter(|(_, c)| &c.name == name);
                let first = matches.next().map(|(i, _)| i);
                if matches.next().is_some() {
                    None
                } else {
                    first
                }
            }
        };
        let value = index.and_then(|i| report.rows.get(extraction.row).and_then(|row| row.get(i)));
        if let Some(value) = value {
            variables.insert(
                extraction.variable.clone(),
                match value {
                    Value::String(s) => s.clone(),
                    value => value.to_string(),
                },
            );
        }
        if extraction.required {
            assertions.push(AssertionResult {
                id: format!("extract.{}", extraction.variable),
                passed: value.is_some(),
                message: if value.is_some() {
                    "value extracted"
                } else {
                    "required result cell is missing or column name is ambiguous"
                }
                .into(),
            });
        }
    }
    let status = if assertions.iter().all(|a| a.passed) {
        OutputStatus::Success
    } else {
        OutputStatus::Failure
    };
    Ok(PluginOutput {
        status,
        data: serde_json::to_value(report)
            .map_err(|_| PluginError::Execution("invalid database report".into()))?,
        variables,
        assertions,
    })
}

#[async_trait]
impl EnvironmentPlugin for DatabaseProvider {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: self.id().into(),
            name: match self {
                Self::Postgres(_) => "PostgreSQL",
                Self::MySql(_) => "MySQL",
            }
            .into(),
            section: "database".into(),
            version: "1.1.0".into(),
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
        let (statement, extract) = if invocation.operation == "test" {
            (
                DatabaseStatement {
                    query: "SELECT 1".into(),
                    mode: StatementMode::Query,
                    parameters: Vec::new(),
                    max_rows: 1,
                },
                Vec::new(),
            )
        } else {
            let input: QueryInput = serde_json::from_value(invocation.input.clone())
                .map_err(|_| PluginError::InvalidConfig("invalid database query input".into()))?;
            input.validate()?;
            (
                DatabaseStatement {
                    query: input.query,
                    mode: input.mode,
                    parameters: input.parameters,
                    max_rows: input.max_rows,
                },
                input.extract,
            )
        };
        statement.validate().map_err(|error| PluginError::InvalidConfig(error.to_string()))?;
        let password = match invocation.config.secrets.get("password") {
            Some(reference) => Some(context.secrets.resolve(&context.scope, reference).await?),
            None => None,
        };
        let target = self.target(&invocation.config, password.as_ref().map(SecretValue::expose))?;
        let report = self
            .query(
                &target,
                &statement,
                QueryBudget {
                    max_bytes: context.limits.max_output_bytes,
                    timeout_ms: context.limits.timeout_ms,
                },
            )
            .await
            .map_err(database_error)?;
        output(report, &extract)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use probe::domain::database::DatabaseColumn;
    use serde_json::json;
    #[test]
    fn extraction_preserves_nulls_and_fails_required_missing_or_ambiguous_cells() {
        let report = DatabaseReport {
            columns: vec![
                DatabaseColumn { name: "id".into(), sql_type: "INT8".into() },
                DatabaseColumn { name: "id".into(), sql_type: "TEXT".into() },
            ],
            rows: vec![vec![json!("9007199254740993"), Value::Null]],
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 1,
        };
        let input: QueryInput = serde_json::from_value(json!({"query":"SELECT 1", "extract":[
            {"variable":"large_id","row":0,"column":0},
            {"variable":"null_value","row":0,"column":1},
            {"variable":"ambiguous","row":0,"column":"id"},
            {"variable":"missing","row":1,"column":0},
            {"variable":"optional","row":9,"column":0,"required":false}
        ]}))
        .expect("input");
        let output = output(report, &input.extract).expect("report");
        assert_eq!(output.status, OutputStatus::Failure);
        assert_eq!(output.variables["large_id"], "9007199254740993");
        assert_eq!(output.variables["null_value"], "null");
        assert!(!output.variables.contains_key("optional"));
        assert_eq!(output.assertions.iter().filter(|a| !a.passed).count(), 2);
        let duplicate: QueryInput = serde_json::from_value(json!({"query":"SELECT 1", "extract":[{"variable":"same","row":0,"column":0},{"variable":"same","row":0,"column":1}]})).expect("input");
        assert!(duplicate.validate().is_err());
        let writing: QueryInput = serde_json::from_value(json!({"query":"INSERT INTO t VALUES (1)","mode":"execute","extract":[{"variable":"v","row":0,"column":0}]})).expect("input");
        assert!(writing.validate().is_err());
    }
}
