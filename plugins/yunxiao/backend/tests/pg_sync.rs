#![cfg(feature = "pg")]
use serde_json::json;
use shepherd_yunxiao::{
    adapters::pg::PgStore,
    domain::{CompleteRun, NewSource, Record},
    ports::Store,
};
fn completion() -> CompleteRun {
    CompleteRun {
        counts: [
            ("requirement".into(), 1),
            ("bug".into(), 0),
            ("sprint".into(), 0),
            ("commit".into(), 0),
        ]
        .into(),
        code_since: "2026-10-21T00:00:00+08:00".into(),
        code_until: "2026-10-28T00:00:00+08:00".into(),
    }
}
#[tokio::test]
#[ignore = "requires isolated SHEPHERD_YUNXIAO_TEST_DATABASE_URL"]
async fn atomic_snapshot_idempotency_history_and_failures() {
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_YUNXIAO_TEST_DATABASE_URL").expect("isolated test database"),
    )
    .await
    .expect("connect");
    shepherd_yunxiao::adapters::pg::migrate(&pool).await.expect("migrate");
    let project = uuid::Uuid::new_v4().to_string();
    let store = PgStore(pool.clone());
    let source = store
        .create_source(NewSource {
            project_id: project.clone(),
            cloud_org_id: "test".into(),
            cloud_project_id: project,
            name: "test".into(),
        })
        .await
        .expect("source");
    let source = source["id"].as_str().expect("id");
    let r = Record {
        kind: "requirement".into(),
        external_id: "req".into(),
        data: json!({"serial":"R-1","title":"Test","status":"开发中","priority":"高","due":"2026-01-01"}),
    };
    let run = store.start(source).await.expect("start");
    let run = run["id"].as_str().expect("id");
    assert!(store.start(source).await.is_err());
    let mut wrong = completion();
    wrong.counts.insert("requirement".into(), 2);
    store.stage(run, vec![r.clone()]).await.expect("stage");
    assert!(store.complete(run, wrong).await.is_err());
    assert_eq!(store.dashboard(source, 7).await.expect("dashboard")["totals"]["requirements"], 0);
    store.complete(run, completion()).await.expect("complete");
    assert_eq!(store.dashboard(source, 7).await.expect("dashboard")["totals"]["requirements"], 1);
    for status in ["开发中", "测试中"] {
        let run = store.start(source).await.expect("start");
        let run = run["id"].as_str().expect("id");
        let mut r = r.clone();
        r.data["status"] = json!(status);
        store.stage(run, vec![r]).await.expect("stage");
        store.complete(run, completion()).await.expect("complete");
    }
    let histories =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM ms_devops_history WHERE source_id=$1")
            .bind(source)
            .fetch_one(&pool)
            .await
            .expect("history");
    assert_eq!(histories, 1);
    let run = store.start(source).await.expect("start");
    store.fail(run["id"].as_str().expect("id"), "test failure").await.expect("fail");
    assert_eq!(store.dashboard(source, 7).await.expect("dashboard")["totals"]["requirements"], 1);
}

#[tokio::test]
#[ignore = "requires isolated SHEPHERD_YUNXIAO_TEST_DATABASE_URL"]
async fn snapshot_stays_in_plugin_database_without_native_tables() {
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_YUNXIAO_TEST_DATABASE_URL").expect("plugin test database"),
    )
    .await
    .expect("connect");
    shepherd_yunxiao::adapters::pg::migrate(&pool).await.expect("plugin migrations");
    let store = PgStore(pool.clone());
    let source = store
        .create_source(NewSource {
            project_id: "opaque-reference-without-native-project".into(),
            cloud_org_id: "test".into(),
            cloud_project_id: uuid::Uuid::new_v4().to_string(),
            name: "isolated".into(),
        })
        .await
        .expect("source does not require a native project");
    let source = source["id"].as_str().expect("source id");
    let mut complete = completion();
    complete.counts.insert("testcase".into(), 1);
    for description in ["original", "changed"] {
        let run = store.start(source).await.expect("start");
        let run = run["id"].as_str().expect("run id");
        store.stage(run, vec![
            Record { kind: "requirement".into(), external_id: "req".into(), data: json!({"title":"requirement","raw":{"description":description}}) },
            Record { kind: "testcase".into(), external_id: "case".into(), data: json!({"title":"case","steps":[{"step":"action","expected":"result"}]}) },
        ]).await.expect("stage");
        store.complete(run, complete.clone()).await.expect("complete");
    }
    let body: String = sqlx::query_scalar("SELECT payload #>> '{raw,description}' FROM ms_devops_entity WHERE source_id=$1 AND kind='requirement'")
        .bind(source).fetch_one(&pool).await.expect("plugin body");
    assert_eq!(body, "changed");
    let native_exists: bool = sqlx::query_scalar("SELECT to_regclass('ms_project') IS NOT NULL OR to_regclass('ms_requirement') IS NOT NULL OR to_regclass('ms_functional_case') IS NOT NULL OR to_regclass('ms_api_module') IS NOT NULL")
        .fetch_one(&pool).await.expect("native table check");
    assert!(!native_exists, "plugin migrations must not install native tables");
}
