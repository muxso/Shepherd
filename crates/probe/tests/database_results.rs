#![cfg(any(feature = "sql", feature = "mysql"))]
use probe::{
    domain::database::DatabaseStatement,
    plugins::database::{DatabaseError, QueryBudget},
};
use serde_json::{json, Value};
fn statement(value: Value) -> DatabaseStatement {
    serde_json::from_value(value).expect("statement")
}
fn budget() -> QueryBudget {
    QueryBudget { max_bytes: 65_536, timeout_ms: 5_000 }
}

#[cfg(feature = "sql")]
#[tokio::test]
#[ignore = "requires disposable ENVIRONMENT_PLUGIN_TEST_DATABASE_URL"]
async fn postgres_typed_results_parameters_limits_and_transaction_lifecycle() {
    let target =
        std::env::var("ENVIRONMENT_PLUGIN_TEST_DATABASE_URL").expect("disposable PostgreSQL URL");
    let driver = probe::plugins::SqlPlugin::new();
    let input = statement(
        json!({"query":"SELECT $1::text AS value, $2::bigint AS nullable, 9007199254740993::bigint AS precise, 1.234567890123456789::numeric AS decimal, '{\"ok\":true}'::jsonb AS document, decode('00ff','hex') AS bytes, DATE '2026-01-02' AS date", "parameters":[{"type":"text","value":"'; DROP TABLE forbidden; --"},{"type":"integer","value":null}]}),
    );
    let result = driver.query(&target, &input, budget()).await.expect("typed query");
    assert_eq!(
        result.rows[0],
        vec![
            json!("'; DROP TABLE forbidden; --"),
            Value::Null,
            json!("9007199254740993"),
            json!("1.234567890123456789"),
            json!({"ok":true}),
            json!("00ff"),
            json!("2026-01-02")
        ]
    );
    let empty = driver
        .query(&target, &statement(json!({"query":"SELECT 1 AS item WHERE false"})), budget())
        .await
        .expect("empty result");
    assert!(empty.rows.is_empty());
    assert_eq!(empty.columns[0].name, "item");
    let duplicate = driver
        .query(&target, &statement(json!({"query":"SELECT 1 AS id, 2 AS id"})), budget())
        .await
        .expect("duplicate column names");
    assert_eq!(duplicate.columns.len(), 2);
    assert_eq!(duplicate.rows[0], vec![json!(1), json!(2)]);
    let limited = driver
        .query(
            &target,
            &statement(json!({"query":"SELECT n FROM generate_series(1,10) n","maxRows":2})),
            budget(),
        )
        .await
        .expect("bounded prefix");
    assert!(limited.truncated);
    assert_eq!(limited.rows.len(), 2);
    assert!(matches!(
        driver
            .query(
                &target,
                &statement(json!({"query":"SELECT repeat('x',1000)"})),
                QueryBudget { max_bytes: 128, ..budget() }
            )
            .await,
        Err(DatabaseError::OutputLimit)
    ));
    assert!(matches!(
        driver
            .query(
                &target,
                &statement(json!({"query":"SELECT pg_sleep(2)"})),
                QueryBudget { timeout_ms: 50, ..budget() }
            )
            .await,
        Err(DatabaseError::Timeout)
    ));
    let execute = driver
        .query(
            &target,
            &statement(
                json!({"query":"CREATE TABLE typed_result_write_test(id BIGINT)","mode":"execute"}),
            ),
            budget(),
        )
        .await
        .expect("create fixture");
    assert_eq!(execute.rows_affected, 0);
    assert!(
        driver
            .query(
                &target,
                &statement(
                    json!({"query":"INSERT INTO typed_result_write_test VALUES (1) RETURNING id"})
                ),
                budget()
            )
            .await
            .is_err(),
        "query mode must be read only"
    );
    let inserted = driver.query(&target, &statement(json!({"query":"INSERT INTO typed_result_write_test VALUES ($1)","parameters":[{"type":"integer","value":42}],"mode":"execute"})),budget()).await.expect("write mode");
    assert_eq!(inserted.rows_affected, 1);
    let persisted = driver
        .query(
            &target,
            &statement(json!({"query":"SELECT id FROM typed_result_write_test"})),
            budget(),
        )
        .await
        .expect("committed insert");
    assert_eq!(persisted.rows[0][0], "42");
    driver
        .query(
            &target,
            &statement(json!({"query":"DROP TABLE typed_result_write_test","mode":"execute"})),
            budget(),
        )
        .await
        .expect("cleanup");
}

#[cfg(feature = "mysql")]
#[tokio::test]
#[ignore = "requires disposable MYSQL_URL (MySQL 8)"]
async fn mysql_typed_results_parameters_limits_and_transaction_lifecycle() {
    let target = std::env::var("MYSQL_URL").expect("disposable MySQL URL");
    let driver = probe::plugins::MysqlPlugin::new();
    let input = statement(
        json!({"query":"SELECT CAST(? AS CHAR) AS value, CAST(? AS SIGNED) AS nullable, CAST(9007199254740993 AS SIGNED) AS precise, CAST(1.23456789 AS DECIMAL(20,8)) AS decimal_value, CAST('{\"ok\":true}' AS JSON) AS document, UNHEX('00ff') AS bytes, CAST('2026-01-02' AS DATE) AS date_value", "parameters":[{"type":"text","value":"'; DROP TABLE forbidden; --"},{"type":"integer","value":null}]}),
    );
    let result = driver.query(&target, &input, budget()).await.expect("typed query");
    assert_eq!(
        result.rows[0],
        vec![
            json!("'; DROP TABLE forbidden; --"),
            Value::Null,
            json!("9007199254740993"),
            json!("1.23456789"),
            json!({"ok":true}),
            json!("00ff"),
            json!("2026-01-02")
        ]
    );
    let empty = driver
        .query(&target, &statement(json!({"query":"SELECT 1 AS item WHERE false"})), budget())
        .await
        .expect("empty result");
    assert!(empty.rows.is_empty());
    assert_eq!(empty.columns[0].name, "item");
    let limited = driver.query(&target, &statement(json!({"query":"SELECT 1 AS item UNION ALL SELECT 2 UNION ALL SELECT 3","maxRows":2})),budget()).await.expect("bounded prefix");
    assert!(limited.truncated);
    assert_eq!(limited.rows.len(), 2);
    assert!(matches!(
        driver
            .query(
                &target,
                &statement(json!({"query":"SELECT REPEAT('x',1000)"})),
                QueryBudget { max_bytes: 128, ..budget() }
            )
            .await,
        Err(DatabaseError::OutputLimit)
    ));
    assert!(matches!(
        driver
            .query(
                &target,
                &statement(json!({"query":"SELECT SLEEP(2)"})),
                QueryBudget { timeout_ms: 50, ..budget() }
            )
            .await,
        Err(DatabaseError::Timeout)
    ));
    driver
        .query(
            &target,
            &statement(
                json!({"query":"CREATE TABLE typed_result_write_test(id BIGINT)","mode":"execute"}),
            ),
            budget(),
        )
        .await
        .expect("create fixture");
    assert!(
        driver
            .query(
                &target,
                &statement(json!({"query":"INSERT INTO typed_result_write_test VALUES (1)"})),
                budget()
            )
            .await
            .is_err(),
        "query mode must be read only"
    );
    let inserted = driver.query(&target, &statement(json!({"query":"INSERT INTO typed_result_write_test VALUES (?)","parameters":[{"type":"integer","value":42}],"mode":"execute"})),budget()).await.expect("write mode");
    assert_eq!(inserted.rows_affected, 1);
    let persisted = driver
        .query(
            &target,
            &statement(json!({"query":"SELECT id FROM typed_result_write_test"})),
            budget(),
        )
        .await
        .expect("committed insert");
    assert_eq!(persisted.rows[0][0], "42");
    driver
        .query(
            &target,
            &statement(json!({"query":"DROP TABLE typed_result_write_test","mode":"execute"})),
            budget(),
        )
        .await
        .expect("cleanup");
}
