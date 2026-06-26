#![cfg(feature = "redis")]

use probe::{default_registry, ProbeAssertion, ProbeRequest};

fn redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/0".into())
}

fn req(cmd: &str, assertions: Vec<ProbeAssertion>) -> ProbeRequest {
    ProbeRequest {
        protocol: "redis".into(),
        target: redis_url(),
        payload: Some(cmd.into()),
        metadata: Default::default(),
        assertions,
    }
}

#[tokio::test]
#[ignore = "需要 Redis"]
async fn ping_set_get_through_registry() {
    let reg = default_registry();
    assert!(reg.protocols().contains(&"redis".to_string()), "redis 插件应在注册表");

    let out = reg
        .dispatch(&req("PING", vec![ProbeAssertion::Success, ProbeAssertion::OutputEquals("PONG".into())]))
        .await;
    assert!(out.success, "PING 应成功: {out:?}");

    let out = reg.dispatch(&req("SET shep_probe_k hello", vec![ProbeAssertion::OutputEquals("OK".into())])).await;
    assert!(out.success, "SET 应回 OK: {out:?}");

    let out = reg.dispatch(&req("GET shep_probe_k", vec![ProbeAssertion::OutputContains("hello".into())])).await;
    assert!(out.success, "GET 应回 hello: {out:?}");
    assert_eq!(out.output.as_deref(), Some("hello"));

    let out = reg.dispatch(&req("GET shep_probe_absent", vec![ProbeAssertion::OutputContains("x".into())])).await;
    assert!(!out.success, "缺失键不应命中断言: {out:?}");
}
