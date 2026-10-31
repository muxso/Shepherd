use super::{DatabaseError, QueryBudget, ReportBuilder};
use crate::domain::database::{
    DatabaseColumn, DatabaseReport, DatabaseStatement, SqlParameter, StatementMode,
};
use futures_util::TryStreamExt;
use serde_json::{json, Value};
use sqlx::{
    postgres::{PgConnectOptions, PgRow},
    types::{
        chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc},
        BigDecimal, Json, Uuid,
    },
    Column, ConnectOptions, Connection, Executor, PgConnection, Row, Statement, Type, TypeInfo,
    ValueRef,
};

pub(super) async fn run(
    target: &str,
    request: &DatabaseStatement,
    budget: QueryBudget,
) -> Result<DatabaseReport, DatabaseError> {
    let options = target.parse::<PgConnectOptions>()?.disable_statement_logging();
    let mut connection = PgConnection::connect_with(&options).await?;
    let mut transaction = connection.begin().await?;
    if request.mode == StatementMode::Query {
        sqlx::query("SET TRANSACTION READ ONLY").execute(&mut *transaction).await?;
    }
    sqlx::query("SELECT set_config('statement_timeout', $1, true)")
        .bind(format!("{}ms", budget.timeout_ms))
        .execute(&mut *transaction)
        .await?;
    let parameter_types = request
        .parameters
        .iter()
        .map(|parameter| match parameter {
            SqlParameter::Text(_) => <String as Type<sqlx::Postgres>>::type_info(),
            SqlParameter::Integer(_) => <i64 as Type<sqlx::Postgres>>::type_info(),
            SqlParameter::Float(_) => <f64 as Type<sqlx::Postgres>>::type_info(),
            SqlParameter::Boolean(_) => <bool as Type<sqlx::Postgres>>::type_info(),
        })
        .collect::<Vec<_>>();
    let statement = transaction.prepare_with(&request.query, &parameter_types).await?;
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
fn cell(row: &PgRow, index: usize) -> Result<Value, DatabaseError> {
    if row.try_get_raw(index)?.is_null() {
        return Ok(Value::Null);
    }
    let kind = row.column(index).type_info().name();
    let value = match kind {
        "BOOL" => json!(row.try_get::<bool, _>(index)?),
        "INT2" => json!(row.try_get::<i16, _>(index)?),
        "INT4" => json!(row.try_get::<i32, _>(index)?),
        // Preserve precision beyond JavaScript's integer range.
        "INT8" => json!(row.try_get::<i64, _>(index)?.to_string()),
        "FLOAT4" => finite(row.try_get::<f32, _>(index)? as f64),
        "FLOAT8" => finite(row.try_get::<f64, _>(index)?),
        "NUMERIC" => json!(row.try_get::<BigDecimal, _>(index)?.normalized().to_string()),
        "TEXT" | "VARCHAR" | "CHAR" | "BPCHAR" | "NAME" | "UNKNOWN" => {
            json!(row.try_get::<String, _>(index)?)
        }
        "JSON" | "JSONB" => row.try_get::<Json<Value>, _>(index)?.0,
        "UUID" => json!(row.try_get::<Uuid, _>(index)?.to_string()),
        "DATE" => json!(row.try_get::<NaiveDate, _>(index)?.to_string()),
        "TIME" => json!(row.try_get::<NaiveTime, _>(index)?.to_string()),
        "TIMESTAMP" => json!(row.try_get::<NaiveDateTime, _>(index)?.to_string()),
        "TIMESTAMPTZ" => {
            json!(row.try_get::<DateTime<Utc>, _>(index)?.to_rfc3339())
        }
        "BYTEA" => json!(super::hex(&row.try_get::<Vec<u8>, _>(index)?)),
        _ => return Err(DatabaseError::UnsupportedType(kind.into())),
    };
    Ok(value)
}
fn finite(value: f64) -> Value {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or_else(|| Value::String(value.to_string()))
}
