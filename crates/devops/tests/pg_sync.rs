#![cfg(feature = "pg")]
use devops::{
    adapters::pg::PgStore,
    domain::{CompleteRun, NewSource, Record},
    ports::Store,
};
use serde_json::json;
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
#[ignore = "requires isolated SHEPHERD_TEST_DATABASE_URL"]
async fn atomic_snapshot_idempotency_history_and_failures() {
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_TEST_DATABASE_URL").expect("isolated test database"),
    )
    .await
    .expect("connect");
    migrate::run(&pool).await.expect("migrate");
    let project = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO ms_project(id,organization_id,name) VALUES($1,'test-org',$1)")
        .bind(&project)
        .execute(&pool)
        .await
        .expect("project");
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
