#![cfg(feature = "pg")]
use serde_json::json;
use shepherd_yunxiao::{
    adapters::pg::PgStore,
    domain::{CompleteRun, ListQuery, NewSource, Record},
    ports::Store,
};

#[tokio::test]
#[ignore = "requires isolated SHEPHERD_YUNXIAO_TEST_DATABASE_URL and SHEPHERD_STATISTICS_TEST_REDIS_URL"]
async fn cache_invalidation_projection_rollback_and_large_record_pagination() {
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_YUNXIAO_TEST_DATABASE_URL").expect("isolated database"),
    )
    .await
    .expect("pool");
    shepherd_yunxiao::adapters::pg::migrate(&pool).await.expect("migrate");
    let url = std::env::var("SHEPHERD_STATISTICS_TEST_REDIS_URL").expect("isolated Redis");
    let store = PgStore::new(pool.clone()).with_redis_url(Some(&url));
    let project = uuid::Uuid::new_v4().to_string();
    let source = store
        .create_source(NewSource {
            project_id: project.clone(),
            cloud_org_id: "fixture".into(),
            cloud_project_id: project,
            name: "Fixture".into(),
        })
        .await
        .expect("source");
    let id = source["id"].as_str().expect("id");
    assert_eq!(store.dashboard(id, 7).await.expect("empty")["totals"]["requirements"], 0);
    let description = "large fixture ".repeat(100_000);
    let run = store.start(id).await.expect("run");
    let run = run["id"].as_str().expect("run id");
    store.stage(run,vec![Record{kind:"requirement".into(),external_id:"req".into(),data:json!({"title":"Fixture","description":description,"status":"开发中","raw":{"description":description,"creator":{"name":"Product A"}}})}]).await.expect("stage");
    assert_eq!(
        store.dashboard(id, 7).await.expect("staged invisible")["totals"]["requirements"],
        0
    );
    let body = CompleteRun {
        counts: [
            ("requirement".into(), 1),
            ("bug".into(), 0),
            ("sprint".into(), 0),
            ("commit".into(), 0),
        ]
        .into(),
        code_since: "2026-01-01T00:00:00Z".into(),
        code_until: "2026-12-01T00:00:00Z".into(),
    };
    store.complete(run, body).await.expect("complete");
    assert_eq!(store.dashboard(id, 7).await.expect("fresh")["totals"]["requirements"], 1);
    let small:i64=sqlx::query_scalar("SELECT octet_length(payload::text)::bigint FROM ms_devops_statistics_entity WHERE source_id=$1 AND kind='requirement'").bind(id).fetch_one(&pool).await.expect("projection size");
    assert!(small < 1000);
    let list = store.records(id, ListQuery::default()).await.expect("page");
    assert_eq!(list["items"][0]["description"], description);
    assert_eq!(list["items"][0]["creator"], "Product A");
    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM ms_devops_statistics_state WHERE source_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("revision");
    let mut tx = pool.begin().await.expect("transaction");
    sqlx::query("UPDATE ms_devops_entity SET active=false WHERE source_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .expect("uncommitted update");
    assert_eq!(
        store.dashboard(id, 7).await.expect("committed snapshot")["totals"]["requirements"],
        1
    );
    tx.rollback().await.expect("rollback");
    let after: i64 =
        sqlx::query_scalar("SELECT revision FROM ms_devops_statistics_state WHERE source_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("revision");
    assert_eq!(revision, after);
    let client = redis::Client::open(url).expect("client");
    let mut redis = client.get_multiplexed_async_connection().await.expect("Redis");
    let key = "shepherd:yunxiao:statistics:v1:*";
    let keys: Vec<String> =
        redis::cmd("KEYS").arg(key).query_async(&mut redis).await.expect("isolated cache keys");
    assert!(!keys.is_empty());
    for key in keys {
        let ttl: i64 =
            redis::cmd("TTL").arg(key).query_async(&mut redis).await.expect("bounded TTL");
        assert!((0..=90).contains(&ttl));
    }
    for _ in 0..200 {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ms_devops_statistics_snapshot WHERE source_id=$1 AND revision=(SELECT revision FROM ms_devops_statistics_state WHERE source_id=$1)",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("precomputation");
        if n >= 5 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ms_devops_statistics_snapshot WHERE source_id=$1 AND revision=(SELECT revision FROM ms_devops_statistics_state WHERE source_id=$1)")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("snapshots");
    assert!(n >= 5);
    let mut blocked = pool.begin().await.expect("block projections");
    sqlx::query("LOCK TABLE ms_devops_statistics_entity IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocked)
        .await
        .expect("lock");
    let fallback = PgStore::new(pool.clone()).with_redis_url(Some("redis://127.0.0.1:1/"));
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(2), fallback.dashboard(id, 7))
            .await
            .expect("precomputed fallback avoids projection lock")
            .expect("database fallback")["totals"]["requirements"],
        1
    );
    blocked.rollback().await.expect("unlock");
}
