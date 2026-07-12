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
#[ignore = "需要 MySQL"]
async fn select_and_ddl_through_registry() {
    let reg = default_registry();
    assert!(reg.protocols().contains(&"mysql".to_string()), "mysql 插件应在注册表");

    let out = reg.dispatch(&req("SELECT 1", vec![ProbeAssertion::Success])).await;
    assert!(out.success, "SELECT 1 应成功: {out:?}");

    let _ = reg.dispatch(&req("DROP TABLE IF EXISTS shep_probe_t", vec![])).await;
    let out = reg
        .dispatch(&req("CREATE TABLE shep_probe_t (id INT)", vec![ProbeAssertion::Success]))
        .await;
    assert!(out.success, "建表应成功: {out:?}");
    let out = reg
        .dispatch(&req(
            "INSERT INTO shep_probe_t VALUES (1)",
            vec![ProbeAssertion::OutputContains("rows_affected=1".into())],
        ))
        .await;
    assert!(out.success, "插入一行应 rows_affected=1: {out:?}");

    let out = reg.dispatch(&req("NOT SQL", vec![])).await;
    assert!(!out.success, "非法语句应失败: {out:?}");

    let _ = reg.dispatch(&req("DROP TABLE IF EXISTS shep_probe_t", vec![])).await;
}
