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

#[tokio::test]
#[ignore = "requires isolated SHEPHERD_TEST_DATABASE_URL"]
async fn native_projection_versions_cases_and_owned_deletion() {
    let pool =
        sqlx::PgPool::connect(&std::env::var("SHEPHERD_TEST_DATABASE_URL").expect("test db"))
            .await
            .expect("connect");
    migrate::run(&pool).await.expect("migrate");
    let project = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO ms_project(id,organization_id,name) VALUES($1,'test-org',$1)")
        .bind(&project)
        .execute(&pool)
        .await
        .expect("project");
    sqlx::query("INSERT INTO ms_requirement(id,project_id,title) VALUES($1,$2,'manual')")
        .bind(format!("manual-{project}"))
        .bind(&project)
        .execute(&pool)
        .await
        .expect("manual");
    let store = PgStore(pool.clone());
    let source = store
        .create_source(NewSource {
            project_id: project.clone(),
            cloud_org_id: "test".into(),
            cloud_project_id: project.clone(),
            name: "test".into(),
        })
        .await
        .expect("source");
    let source = source["id"].as_str().expect("id");
    let mut req = Record {
        kind: "requirement".into(),
        external_id: "req1".into(),
        data: json!({"serial":"R-1","title":"需求","status":"测试中","priority":"高","createdAt":"2026-10-01T00:00:00+08:00","updatedAt":"2026-10-28T00:00:00+08:00","raw":{"description":"original"}}),
    };
    let mut case = Record {
        kind: "testcase".into(),
        external_id: "repo:case1".into(),
        data: json!({"serial":"T-1","title":"测试用例","modulePath":["云效测试库","库","目录"],"priority":"P1","repo":"库","precondition":"前置","steps":[{"step":"操作","expected":"预期"}],"createdAt":"2026-10-01T00:00:00+08:00","updatedAt":"2026-10-28T00:00:00+08:00"}),
    };
    let mut complete = completion();
    complete.counts.insert("testcase".into(), 1);
    for i in 0..3 {
        if i == 2 {
            req.data["raw"]["description"] = json!("changed");
            case.data["creator"] = json!("云效创建人");
        }
        let run = store.start(source).await.expect("start");
        let run = run["id"].as_str().expect("id");
        store.stage(run, vec![req.clone(), case.clone()]).await.expect("stage");
        store.complete(run, complete.clone()).await.expect("complete");
    }
    let versions=sqlx::query_scalar::<_,i64>("SELECT count(*) FROM ms_requirement_version v JOIN ms_requirement r ON r.id=v.requirement_id WHERE r.external_source_id=$1").bind(source).fetch_one(&pool).await.expect("versions");
    assert_eq!(versions, 2);
    let first=sqlx::query_scalar::<_,String>("SELECT description FROM ms_requirement_version v JOIN ms_requirement r ON r.id=v.requirement_id WHERE r.external_source_id=$1 AND v.version=1").bind(source).fetch_one(&pool).await.expect("original version");
    assert_eq!(first, "original");
    let num = sqlx::query_scalar::<_, i64>(
        "SELECT num FROM ms_functional_case WHERE external_source_id=$1",
    )
    .bind(source)
    .fetch_one(&pool)
    .await
    .expect("case");
    assert_eq!(num, 100001);
    let pre = sqlx::query_scalar::<_, String>(
        "SELECT custom_fields->>'前置条件' FROM ms_functional_case WHERE external_source_id=$1",
    )
    .bind(source)
    .fetch_one(&pool)
    .await
    .expect("precondition");
    assert_eq!(pre, "前置");
    let creator = sqlx::query_scalar::<_, String>(
        "SELECT created_by FROM ms_functional_case WHERE external_source_id=$1",
    )
    .bind(source)
    .fetch_one(&pool)
    .await
    .expect("updated source creator");
    assert_eq!(creator, "云效创建人");
    let module_exists=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM ms_functional_case c JOIN ms_api_module m ON m.id=c.module WHERE c.external_source_id=$1 AND m.name='目录')").bind(source).fetch_one(&pool).await.expect("directory");
    assert!(module_exists);
    // Legacy collectors without testcase counts must leave imported cases intact.
    let run = store.start(source).await.expect("start");
    let run = run["id"].as_str().expect("id");
    store.stage(run, vec![req]).await.expect("stage");
    store.complete(run, completion()).await.expect("legacy");
    let n = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM ms_functional_case WHERE external_source_id=$1 AND NOT deleted",
    )
    .bind(source)
    .fetch_one(&pool)
    .await
    .expect("cases");
    assert_eq!(n, 1);
    let mut empty = complete;
    for v in empty.counts.values_mut() {
        *v = 0;
    }
    let run = store.start(source).await.expect("start");
    store.complete(run["id"].as_str().expect("id"), empty).await.expect("empty snapshot");
    let native = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM ms_requirement WHERE project_id=$1 AND NOT deleted",
    )
    .bind(&project)
    .fetch_one(&pool)
    .await
    .expect("native preserved");
    assert_eq!(native, 1);
}
