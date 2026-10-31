//! Bounded result collection. Each call owns its connection and transaction.
#[cfg(feature = "mysql")]
mod mysql;
#[cfg(feature = "sql")]
mod postgres;

use crate::domain::database::{DatabaseColumn, DatabaseReport, StatementError};
use serde_json::Value;
use std::time::Instant;

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error(transparent)]
    InvalidRequest(#[from] StatementError),
    #[error("invalid query budget")]
    InvalidBudget,
    #[error("database operation timed out")]
    Timeout,
    #[error("database operation failed: {0}")]
    Driver(#[source] sqlx::Error),
    #[error("unsupported database result type: {0}; cast the column to text")]
    UnsupportedType(String),
    #[error("database result exceeds output limit")]
    OutputLimit,
}

impl From<sqlx::Error> for DatabaseError {
    fn from(error: sqlx::Error) -> Self {
        if let Some(database) = error.as_database_error() {
            if database.code().as_deref() == Some("57014") {
                return Self::Timeout;
            }
            #[cfg(feature = "mysql")]
            if database
                .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
                .is_some_and(|error| error.number() == 3024)
            {
                return Self::Timeout;
            }
        }
        Self::Driver(error)
    }
}

/// The caller's runtime owns the deadline and cancellation; drivers also set server limits.
#[derive(Clone, Copy)]
pub struct QueryBudget {
    pub max_bytes: usize,
    pub timeout_ms: u64,
}
impl QueryBudget {
    fn validate(self) -> Result<(), DatabaseError> {
        if !(1..=16_777_216).contains(&self.max_bytes) || !(1..=600_000).contains(&self.timeout_ms)
        {
            return Err(DatabaseError::InvalidBudget);
        }
        Ok(())
    }
}

struct ReportBuilder {
    report: DatabaseReport,
    max_rows: usize,
    remaining_bytes: usize,
}
impl ReportBuilder {
    fn new(
        columns: Vec<DatabaseColumn>,
        max_rows: usize,
        budget: QueryBudget,
    ) -> Result<Self, DatabaseError> {
        let report = DatabaseReport {
            columns,
            rows: Vec::new(),
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 0,
        };
        // Reserve growth of elapsedMs/rowsAffected and account for row separators below.
        let metadata_bytes =
            serde_json::to_vec(&report).map_err(|_| DatabaseError::OutputLimit)?.len() + 32;
        let remaining_bytes =
            budget.max_bytes.checked_sub(metadata_bytes).ok_or(DatabaseError::OutputLimit)?;
        Ok(Self { report, max_rows, remaining_bytes })
    }
    fn full(&mut self) -> bool {
        if self.report.rows.len() == self.max_rows {
            self.report.truncated = true;
            true
        } else {
            false
        }
    }
    fn push(&mut self, row: Vec<Value>) -> Result<(), DatabaseError> {
        let bytes = serde_json::to_vec(&row).map_err(|_| DatabaseError::OutputLimit)?.len() + 1;
        self.remaining_bytes =
            self.remaining_bytes.checked_sub(bytes).ok_or(DatabaseError::OutputLimit)?;
        self.report.rows.push(row);
        Ok(())
    }
    fn finish(mut self, rows_affected: u64) -> DatabaseReport {
        self.report.rows_affected = rows_affected;
        self.report
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(DIGITS[(byte >> 4) as usize] as char);
        result.push(DIGITS[(byte & 15) as usize] as char);
    }
    result
}

#[cfg(feature = "sql")]
pub(super) async fn query_postgres(
    target: &str,
    statement: &crate::domain::database::DatabaseStatement,
    budget: QueryBudget,
) -> Result<DatabaseReport, DatabaseError> {
    statement.validate()?;
    budget.validate()?;
    let started = Instant::now();
    let mut report = tokio::time::timeout(
        std::time::Duration::from_millis(budget.timeout_ms),
        postgres::run(target, statement, budget),
    )
    .await
    .map_err(|_| DatabaseError::Timeout)??;
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(report)
}

#[cfg(feature = "mysql")]
pub(super) async fn query_mysql(
    target: &str,
    statement: &crate::domain::database::DatabaseStatement,
    budget: QueryBudget,
) -> Result<DatabaseReport, DatabaseError> {
    statement.validate()?;
    budget.validate()?;
    let started = Instant::now();
    let mut report = tokio::time::timeout(
        std::time::Duration::from_millis(budget.timeout_ms),
        mysql::run(target, statement, budget),
    )
    .await
    .map_err(|_| DatabaseError::Timeout)??;
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn bounded_collection_distinguishes_truncation_from_oversized_output() {
        let columns = vec![DatabaseColumn { name: "value".into(), sql_type: "TEXT".into() }];
        let mut report =
            ReportBuilder::new(columns.clone(), 1, QueryBudget { max_bytes: 256, timeout_ms: 100 })
                .expect("budget");
        assert!(!report.full());
        report.push(vec![json!(null)]).expect("one row");
        assert!(report.full());
        let result = report.finish(0);
        assert!(result.truncated);
        assert_eq!(result.rows, vec![vec![Value::Null]]);
        let mut report =
            ReportBuilder::new(columns, 100, QueryBudget { max_bytes: 256, timeout_ms: 100 })
                .expect("budget");
        assert!(matches!(
            report.push(vec![json!("x".repeat(257))]),
            Err(DatabaseError::OutputLimit)
        ));
    }
}
