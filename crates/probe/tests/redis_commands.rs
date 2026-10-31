#![cfg(feature = "redis")]
use probe::{
    domain::redis::RedisCommand,
    plugins::{
        redis_command::{RedisBudget, RedisCommandError},
        RedisPlugin,
    },
};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
fn command(name: &str, arguments: Value) -> RedisCommand {
    serde_json::from_value(json!({"command":name,"arguments":arguments})).expect("command")
}
fn budget() -> RedisBudget {
    RedisBudget { timeout_ms: 5_000, max_bytes: 65_536 }
}
#[tokio::test]
#[ignore = "requires disposable REDIS_URL (Redis 6+ with ACL)"]
async fn structured_redis_auth_arguments_results_limits_and_connection_cleanup() {
    let target = std::env::var("REDIS_URL").expect("disposable Redis URL");
    let driver = RedisPlugin::new();
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
    let user = format!("ep_typed_{unique}");
    let key = format!("ep_typed:{unique}:key");
    let hash = format!("ep_typed:{unique}:hash");
    let blocked = format!("ep_typed:{unique}:blocked");
    let password = "test password @:/?#";
    driver
        .execute_command(
            &target,
            &command(
                "ACL",
                json!(["SETUSER", user, "on", format!(">{password}"), "~ep_typed:*", "+@all"]),
            ),
            budget(),
        )
        .await
        .expect("ACL fixture");
    let mut authenticated = redis::parse_redis_url(&target).expect("URL");
    authenticated.set_username(&user).expect("username");
    authenticated.set_password(Some(password)).expect("password");
    authenticated.set_path("/13");
    authenticated.query_pairs_mut().append_pair("protocol", "resp3");
    let endpoint = authenticated.as_str();
    let pong = driver
        .execute_command(endpoint, &command("PING", json!([])), budget())
        .await
        .expect("ACL authentication");
    assert_eq!(pong.value, "PONG");
    driver
        .execute_command(endpoint, &command("SET", json!([key, "a value with spaces ☃"])), budget())
        .await
        .expect("set");
    let get = driver
        .execute_command(endpoint, &command("GET", json!([key])), budget())
        .await
        .expect("get");
    assert_eq!(get.value, "a value with spaces ☃");
    let missing = driver
        .execute_command(endpoint, &command("MGET", json!([key, blocked])), budget())
        .await
        .expect("array");
    assert_eq!(missing.value, json!(["a value with spaces ☃", null]));
    driver
        .execute_command(endpoint, &command("SET", json!([key,{"hex":"00ff"}])), budget())
        .await
        .expect("binary set");
    let binary = driver
        .execute_command(endpoint, &command("GET", json!([key])), budget())
        .await
        .expect("binary get");
    assert_eq!(binary.value, json!({"hex":"00ff"}));
    driver
        .execute_command(endpoint, &command("SET", json!([key, "9007199254740992"])), budget())
        .await
        .expect("large value");
    let integer = driver
        .execute_command(endpoint, &command("INCR", json!([key])), budget())
        .await
        .expect("integer");
    assert_eq!(integer.value, "9007199254740993");
    driver
        .execute_command(
            endpoint,
            &command("HSET", json!([hash, "field", "value with spaces"])),
            budget(),
        )
        .await
        .expect("hash");
    let map = driver
        .execute_command(endpoint, &command("HGETALL", json!([hash])), budget())
        .await
        .expect("RESP3 map");
    assert_eq!(map.value, json!([["field", "value with spaces"]]));
    assert!(matches!(
        driver
            .execute_command(
                endpoint,
                &command("ECHO", json!(["x".repeat(1000)])),
                RedisBudget { max_bytes: 128, ..budget() }
            )
            .await,
        Err(RedisCommandError::OutputLimit)
    ));
    assert!(matches!(
        driver
            .execute_command(
                endpoint,
                &command("BLPOP", json!([blocked, "0"])),
                RedisBudget { timeout_ms: 50, ..budget() }
            )
            .await,
        Err(RedisCommandError::Timeout)
    ));
    let client = redis::Client::open(target.clone()).expect("client");
    let mut monitor = client.get_multiplexed_async_connection().await.expect("monitor");
    let mut closed = false;
    for _ in 0..20 {
        let clients: String =
            redis::cmd("CLIENT").arg("LIST").query_async(&mut monitor).await.expect("clients");
        if !clients.contains("cmd=blpop") {
            closed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(closed, "timed out call must drop its owned Redis connection");
    assert!(matches!(
        driver.execute_command(endpoint, &command("NOT_A_COMMAND", json!([])), budget()).await,
        Err(RedisCommandError::Driver(_))
    ));
    let mut wrong = authenticated.clone();
    wrong.set_password(Some("wrong password")).expect("password");
    assert!(driver
        .execute_command(wrong.as_str(), &command("PING", json!([])), budget())
        .await
        .is_err());
    driver
        .execute_command(endpoint, &command("DEL", json!([key, hash])), budget())
        .await
        .expect("key cleanup");
    driver
        .execute_command(&target, &command("ACL", json!(["DELUSER", user])), budget())
        .await
        .expect("ACL cleanup");
}
