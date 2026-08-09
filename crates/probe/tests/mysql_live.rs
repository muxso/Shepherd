#![cfg(feature = "mysql")]

use probe::{default_registry, ProbeAssertion, ProbeRequest};

fn mysql_url() -> String {
    std::env::var("MYSQL_URL")
        .unwrap_or_else(|_| "mysql://root:mspass@127.0.0.1:3306/mstest".into())
}

fn req(stmt: &str, assertions: Vec<ProbeAssertion>) -> ProbeRequest {
    ProbeRequest {
        protocol: "mysql".into(),
        target: mysql_url(),
        payload: Some(stmt.into()),
        metadata: Default::default(),
        assertions,
    }
}

#[tokio::test]
#[ignore = "requires MySQL"]
async fn select_and_ddl_through_registry() {
    let reg = default_registry();
    assert!(
        reg.protocols().contains(&"mysql".to_string()),
        "mysql plugin should be in the registry"
    );

    let out = reg.dispatch(&req("SELECT 1", vec![ProbeAssertion::Success])).await;
    assert!(out.success, "SELECT 1 should succeed: {out:?}");

    let _ = reg.dispatch(&req("DROP TABLE IF EXISTS shep_probe_t", vec![])).await;
    let out = reg
        .dispatch(&req("CREATE TABLE shep_probe_t (id INT)", vec![ProbeAssertion::Success]))
        .await;
    assert!(out.success, "CREATE TABLE should succeed: {out:?}");
    let out = reg
        .dispatch(&req(
            "INSERT INTO shep_probe_t VALUES (1)",
            vec![ProbeAssertion::OutputContains("rows_affected=1".into())],
        ))
        .await;
    assert!(out.success, "inserting one row should give rows_affected=1: {out:?}");

    let out = reg.dispatch(&req("NOT SQL", vec![])).await;
    assert!(!out.success, "an invalid statement should fail: {out:?}");

    let _ = reg.dispatch(&req("DROP TABLE IF EXISTS shep_probe_t", vec![])).await;
}
