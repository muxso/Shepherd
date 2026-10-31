use super::{DatabaseError, QueryBudget, ReportBuilder};
use crate::domain::database::{
    DatabaseColumn, DatabaseReport, DatabaseStatement, SqlParameter, StatementMode,
};
use futures_util::TryStreamExt;
use serde_json::{json, Value};
use sqlx::{
    mysql::{MySqlConnectOptions, MySqlRow},
    types::{
        chrono::{NaiveDate, NaiveDateTime, NaiveTime},
        BigDecimal, Json,
    },
    Column, ConnectOptions, Connection, Executor, MySqlConnection, Row, Statement, TypeInfo,
    ValueRef,
};

pub(super) async fn run(
    target: &str,
    request: &DatabaseStatement,
    budget: QueryBudget,
) -> Result<DatabaseReport, DatabaseError> {
    let options = target.parse::<MySqlConnectOptions>()?.disable_statement_logging();
    let mut connection = MySqlConnection::connect_with(&options).await?;
    sqlx::query(&format!("SET SESSION MAX_EXECUTION_TIME = {}", budget.timeout_ms))
        .execute(&mut connection)
        .await?;
    if request.mode == StatementMode::Query {
        sqlx::query("SET TRANSACTION READ ONLY").execute(&mut connection).await?;
    }
    let mut transaction = connection.begin().await?;
    let statement = transaction.prepare(&request.query).await?;
    let columns = statement
        .columns()
        .iter()
        .map(|c| DatabaseColumn { name: c.name().into(), sql_type: c.type_info().name().into() })
        .collect();
    let mut report = ReportBuilder::new(columns, request.max_rows, budget)?;
    let mut query = statement.query();
    for parameter in &request.parameters {
        query = match parameter {
            SqlParameter::Text(v) => query.bind(v.as_deref()),
            SqlParameter::Integer(v) => query.bind(*v),
            SqlParameter::Float(v) => query.bind(*v),
            SqlParameter::Boolean(v) => query.bind(*v),
        };
    }
    let affected = if request.mode == StatementMode::Execute {
        query.execute(&mut *transaction).await?.rows_affected()
    } else {
        let mut stream = query.fetch(&mut *transaction);
        while let Some(row) = stream.try_next().await? {
            if report.full() {
                break;
            }
            report
                .push((0..row.len()).map(|index| cell(&row, index)).collect::<Result<_, _>>()?)?;
        }
        0
    };
    if report.report.truncated {
        // Dropping the transaction and its owned connection closes the result stream.
        return Ok(report.finish(affected));
    }
    if request.mode == StatementMode::Execute {
        transaction.commit().await?;
    } else {
        transaction.rollback().await?;
    }
    Ok(report.finish(affected))
}
fn cell(row: &MySqlRow, index: usize) -> Result<Value, DatabaseError> {
    if row.try_get_raw(index)?.is_null() {
        return Ok(Value::Null);
    }
    let kind = row.column(index).type_info().name();
    let value = match kind {
        "BOOLEAN" => json!(row.try_get::<bool, _>(index)?),
        "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" => {
            json!(row.try_get::<i64, _>(index)?)
        }
        "BIGINT" => json!(row.try_get::<i64, _>(index)?.to_string()),
        "TINYINT UNSIGNED" | "SMALLINT UNSIGNED" | "MEDIUMINT UNSIGNED" | "INT UNSIGNED" => {
            json!(row.try_get::<u64, _>(index)?)
        }
        "BIGINT UNSIGNED" => json!(row.try_get::<u64, _>(index)?.to_string()),
        "FLOAT" => finite(row.try_get::<f32, _>(index)? as f64),
        "DOUBLE" => finite(row.try_get::<f64, _>(index)?),
        "DECIMAL" => json!(row.try_get::<BigDecimal, _>(index)?.normalized().to_string()),
        "CHAR" | "VARCHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM" | "SET" => {
            json!(row.try_get::<String, _>(index)?)
        }
        "JSON" => row.try_get::<Json<Value>, _>(index)?.0,
        "DATE" => json!(row.try_get::<NaiveDate, _>(index)?.to_string()),
        "TIME" => json!(row.try_get::<NaiveTime, _>(index)?.to_string()),
        "TIMESTAMP" | "DATETIME" => {
            json!(row.try_get::<NaiveDateTime, _>(index)?.to_string())
        }
        "BINARY" | "VARBINARY" | "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" => {
            json!(super::hex(&row.try_get::<Vec<u8>, _>(index)?))
        }
        _ => return Err(DatabaseError::UnsupportedType(kind.into())),
    };
    Ok(value)
}
fn finite(value: f64) -> Value {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or_else(|| Value::String(value.to_string()))
}
