//! Redis 活体集成测试:经注册表对真实 Redis 跑 PING / SET / GET,验证断言判定。
//!
//! 需要 Redis,故 `#[ignore]`;运行:
//!   docker run -d --name shep-redis -p 6379:6379 redis:7-alpine
//!   cargo test -p probe --features redis --test redis_live -- --ignored --test-threads=1
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

    // PING → PONG,断言生效。
    let out = reg
        .dispatch(&req("PING", vec![ProbeAssertion::Success, ProbeAssertion::OutputEquals("PONG".into())]))
        .await;
    assert!(out.success, "PING 应成功: {out:?}");

    // SET → OK。
    let out = reg.dispatch(&req("SET shep_probe_k hello", vec![ProbeAssertion::OutputEquals("OK".into())])).await;
    assert!(out.success, "SET 应回 OK: {out:?}");

    // GET → hello(复用同一缓存连接)。
    let out = reg.dispatch(&req("GET shep_probe_k", vec![ProbeAssertion::OutputContains("hello".into())])).await;
    assert!(out.success, "GET 应回 hello: {out:?}");
    assert_eq!(out.output.as_deref(), Some("hello"));

    // 断言不满足 → 失败(GET 一个不存在的键期望非空)。
    let out = reg.dispatch(&req("GET shep_probe_absent", vec![ProbeAssertion::OutputContains("x".into())])).await;
    assert!(!out.success, "缺失键不应命中断言: {out:?}");
}
