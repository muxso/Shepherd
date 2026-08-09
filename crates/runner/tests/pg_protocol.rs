#![cfg(feature = "pg")]

use runner::adapters::pg::PgRunnerAgentStore;
use runner::domain::NewRunnerAgent;
use runner::ports::RunnerAgentStore;

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://msuser:mspass@localhost:55432/mstest".into())
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn protocols_column_roundtrip_and_protocol_routing() {
    let pool = migrate::connect(&db_url()).await.expect("connect PG");
    migrate::run(&pool).await.expect("run migrations");
    sqlx::query("DELETE FROM ms_runner_agent WHERE name LIKE 'pgtest-%'")
        .execute(&pool)
        .await
        .expect("cleanup");

    let store = PgRunnerAgentStore::new(pool.clone());

    let grpc_agent =
        NewRunnerAgent::new("pgtest-grpc", "http://grpc-env:9100", None, true).unwrap();
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

    let listed = store.list().await.expect("list");
    let g_view = listed.iter().find(|a| a.name == "pgtest-grpc").expect("grpc in list");
    assert!(g_view.protocols.contains(&"grpc".to_string()));

    let grpc_cands = store.agents_for_protocol("grpc").await.expect("by grpc");
    assert_eq!(grpc_cands.len(), 1);
    assert_eq!(grpc_cands[0].id, g.id);

    let http_cands = store.agents_for_protocol("http").await.expect("by http");
    assert_eq!(http_cands.len(), 2);

    assert!(store.agents_for_protocol("redis").await.expect("by redis").is_empty());

    assert!(store
        .set_protocols(&g.id, &["http".to_string(), "grpc".to_string(), "redis".to_string()])
        .await
        .expect("set_protocols"));
    let redis_cands = store.agents_for_protocol("redis").await.expect("by redis after refresh");
    assert_eq!(redis_cands.len(), 1);
    assert_eq!(redis_cands[0].id, g.id);

    assert!(!store.set_protocols("ghost", &[]).await.expect("set ghost"));

    sqlx::query("DELETE FROM ms_runner_agent WHERE name LIKE 'pgtest-%'")
        .execute(&pool)
        .await
        .expect("cleanup");
}
