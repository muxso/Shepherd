//! Typed SQL boundary shared by database engines. No driver or network dependencies.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatementMode {
    #[default]
    Query,
    Execute,
}

/// NULL keeps its SQL type; callers never interpolate parameters into SQL text.
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum SqlParameter {
    Text(Option<String>),
    Integer(Option<i64>),
    Float(Option<f64>),
    Boolean(Option<bool>),
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatabaseStatement {
    pub query: String,
    #[serde(default)]
    pub mode: StatementMode,
    #[serde(default)]
    pub parameters: Vec<SqlParameter>,
    #[serde(default = "default_max_rows")]
    pub max_rows: usize,
}
fn default_max_rows() -> usize {
    100
}
impl DatabaseStatement {
    pub fn validate(&self) -> Result<(), StatementError> {
        if self.query.trim().is_empty() || self.query.len() > 65_536 {
            return Err(StatementError::InvalidQuery);
        }
        if !(1..=1_000).contains(&self.max_rows) || self.parameters.len() > 128 {
            return Err(StatementError::InvalidLimits);
        }
        if self.parameters.iter().any(|p| match p {
            SqlParameter::Text(Some(s)) => s.len() > 65_536,
            SqlParameter::Float(Some(n)) => !n.is_finite(),
            _ => false,
        }) {
            return Err(StatementError::InvalidParameter);
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum StatementError {
    #[error("query is empty or too large")]
    InvalidQuery,
    #[error("query limits out of range")]
    InvalidLimits,
    #[error("invalid query parameter")]
    InvalidParameter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseColumn {
    pub name: String,
    pub sql_type: String,
}

/// Rows are positional, preserving duplicate column names and NULL values.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseReport {
    pub columns: Vec<DatabaseColumn>,
    pub rows: Vec<Vec<Value>>,
    pub rows_affected: u64,
    pub truncated: bool,
    pub elapsed_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn statement_validation_rejects_unbounded_or_ambiguous_input() {
        let mut statement: DatabaseStatement = serde_json::from_value(
            json!({"query":"SELECT $1", "parameters":[{"type":"integer","value":null}]}),
        )
        .expect("typed null");
        statement.validate().expect("valid request");
        statement.max_rows = 0;
        assert!(statement.validate().is_err());
        statement.max_rows = 1001;
        assert!(statement.validate().is_err());
        assert!(serde_json::from_value::<DatabaseStatement>(
            json!({"query":"SELECT 1","ignored":true})
        )
        .is_err());
        assert!(serde_json::from_value::<DatabaseStatement>(
            json!({"query":"SELECT 1","parameters":[{"type":"integer","value":"not an integer"}]})
        )
        .is_err());
        assert!(serde_json::from_value::<DatabaseStatement>(
            json!({"query":"SELECT 1","mode":"unknown"})
        )
        .is_err());
    }
}
