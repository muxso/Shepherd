//! PG 集成测试:验证「按协议选 agent」相关的真实 SQL(text[] 列 + `= ANY` 筛选 + 刷新)。
//!
//! 与 in-memory 单测互补:这里跑真库,证明迁移 0032 的 `protocols text[]` 列、
//! `agents_for_protocol` 的 `$1 = ANY(protocols)`、`set_protocols` 更新都成立。
//!
//! 需要 PostgreSQL,故 `#[ignore]`;运行:
//!   docker run -d --name shep-pg -e POSTGRES_USER=msuser -e POSTGRES_PASSWORD=mspass \
//!     -e POSTGRES_DB=mstest -p 55432:5432 postgres:16-alpine
//!   cargo test -p runner --features pg --test pg_protocol -- --ignored --test-threads=1
#![cfg(feature = "pg")]

use runner::adapters::pg::PgRunnerAgentStore;
use runner::domain::NewRunnerAgent;
use runner::ports::RunnerAgentStore;

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://msuser:mspass@localhost:55432/mstest".into())
}

#[tokio::test]
#[ignore = "需要 PostgreSQL"]
async fn protocols_column_roundtrip_and_protocol_routing() {
    let pool = migrate::connect(&db_url()).await.expect("connect PG");
    migrate::run(&pool).await.expect("run migrations");
    // 隔离:清掉本测试要用的注册记录(用独特名字前缀)。
    sqlx::query("DELETE FROM ms_runner_agent WHERE name LIKE 'pgtest-%'")
        .execute(&pool)
        .await
        .expect("cleanup");

    let store = PgRunnerAgentStore::new(pool.clone());

    // 注册两个能力不同的 agent。
    let grpc_agent = NewRunnerAgent::new("pgtest-grpc", "http://grpc-env:9100", None, true).unwrap();
    let g = store
        .insert(&grpc_agent, &["http".to_string(), "grpc".to_string()])
        .await
        .expect("insert grpc agent");
    assert_eq!(g.protocols, vec!["http".to_string(), "grpc".to_string()]);

    let sql_agent = NewRunnerAgent::new("pgtest-sql", "http://sql-env:9100", None, true).unwrap();
    store
        .insert(&sql_agent, &["http".to_string(), "sql".to_string()])
        .await
        .expect("insert sql agent");

    // list 读回 protocols 列。
    let listed = store.list().await.expect("list");
    let g_view = listed.iter().find(|a| a.name == "pgtest-grpc").expect("grpc in list");
    assert!(g_view.protocols.contains(&"grpc".to_string()));

    // 按协议筛选:grpc → 只命中 grpc agent。
    let grpc_cands = store.agents_for_protocol("grpc").await.expect("by grpc");
    assert_eq!(grpc_cands.len(), 1);
    assert_eq!(grpc_cands[0].id, g.id);

    // http → 两个都支持。
    let http_cands = store.agents_for_protocol("http").await.expect("by http");
    assert_eq!(http_cands.len(), 2);

    // redis → 无候选。
    assert!(store.agents_for_protocol("redis").await.expect("by redis").is_empty());

    // 刷新能力:给 grpc agent 加 redis 支持。
    assert!(store
        .set_protocols(&g.id, &["http".to_string(), "grpc".to_string(), "redis".to_string()])
        .await
        .expect("set_protocols"));
    let redis_cands = store.agents_for_protocol("redis").await.expect("by redis after refresh");
    assert_eq!(redis_cands.len(), 1);
    assert_eq!(redis_cands[0].id, g.id);

    // 刷新不存在的 agent → false。
    assert!(!store.set_protocols("ghost", &[]).await.expect("set ghost"));

    // 清理。
    sqlx::query("DELETE FROM ms_runner_agent WHERE name LIKE 'pgtest-%'")
        .execute(&pool)
        .await
        .expect("cleanup");
}
