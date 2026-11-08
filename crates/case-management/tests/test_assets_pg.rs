#![cfg(feature = "pg")]
use case_management::{
    adapters::test_assets_pg::PgAssetStore, domain::test_assets::*, ports::test_assets::AssetStore,
};
use serde_json::{json, Value};
use sqlx::PgPool;
fn case(title: &str) -> SourceRecord {
    SourceRecord {
        kind: "testcase".into(),
        external_id: "library:one".into(),
        data: json!({"title":title,"priority":"P1","repoId":"library","repo":"Source library","directory":"Orders","modulePath":["Source library","Orders"],"steps":[{"step":"Submit","expected":"Accepted"}],"precondition":"Signed in"}),
    }
}
async fn fixture() -> (PgPool, PgAssetStore, String, String) {
    let url = std::env::var("SHEPHERD_ASSETS_TEST_DATABASE_URL").expect("isolated database URL");
    assert!(url.contains("127.0.0.1") && url.contains("test"));
    let pool = PgPool::connect(&url).await.expect("connect");
    migrate::run(&pool).await.expect("migrate");
    let first:String=sqlx::query_scalar("INSERT INTO ms_project(organization_id,name) VALUES('test',gen_random_uuid()::text) RETURNING id").fetch_one(&pool).await.expect("project");
    let second:String=sqlx::query_scalar("INSERT INTO ms_project(organization_id,name) VALUES('test',gen_random_uuid()::text) RETURNING id").fetch_one(&pool).await.expect("project");
    (pool.clone(), PgAssetStore(pool), first, second)
}
#[tokio::test]
#[ignore = "requires isolated SHEPHERD_ASSETS_TEST_DATABASE_URL"]
async fn imports_keep_ids_manual_fields_versions_and_conflicts() {
    let (pool, store, p, _) = fixture().await;
    let mut snap = SourceSnapshot {
        source_id: "source".into(),
        revision: "one".into(),
        records: vec![
            case("Order accepted"),
            SourceRecord {
                kind: "requirement".into(),
                external_id: "req-one".into(),
                data: json!({"serial":"REQ-1","title":"Orders","description":"Initial version"}),
            },
        ],
    };
    let preview = store.preview_import(&p, "reviewer", &snap).await.expect("preview");
    assert_eq!(preview["preview"]["counts"]["CREATE"], 2);
    let id = preview["id"].as_str().expect("batch");
    assert!(store.apply_import(&p, "reviewer", id, "changed", &json!({})).await.is_err());
    let result = store.apply_import(&p, "reviewer", id, "one", &json!({})).await.expect("apply");
    assert_eq!(result["imported"], 2);
    assert_eq!(
        store.apply_import(&p, "reviewer", id, "one", &json!({})).await.expect("idempotent"),
        result
    );
    let native: String = sqlx::query_scalar(
        "SELECT native_id FROM ms_test_asset_origin WHERE project_id=$1 AND kind='testcase'",
    )
    .bind(&p)
    .fetch_one(&pool)
    .await
    .expect("native id");
    sqlx::query("UPDATE ms_functional_case SET custom_fields=custom_fields||'{\"manual-note\":\"Keep this\"}'::jsonb WHERE id=$1").bind(&native).execute(&pool).await.expect("manual field");
    snap.revision = "two".into();
    snap.records[0] = case("Order updated");
    snap.records[1].data["description"] = json!("New source version");
    let b = store.preview_import(&p, "reviewer", &snap).await.expect("updated preview");
    assert_eq!(b["preview"]["counts"]["UPDATE"], 2);
    store
        .apply_import(&p, "reviewer", b["id"].as_str().expect("batch"), "two", &json!({}))
        .await
        .expect("update");
    let fields: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM ms_functional_case c WHERE id=$1")
            .bind(&native)
            .fetch_one(&pool)
            .await
            .expect("case");
    assert_eq!(fields["name"], "Order updated");
    assert_eq!(fields["custom_fields"]["manual-note"], "Keep this");
    let versions:i64=sqlx::query_scalar("SELECT count(*) FROM ms_requirement_version v JOIN ms_requirement r ON r.id=v.requirement_id WHERE r.project_id=$1").bind(&p).fetch_one(&pool).await.expect("versions");
    assert_eq!(versions, 2);
    sqlx::query("UPDATE ms_functional_case SET name='Human edit' WHERE id=$1")
        .bind(&native)
        .execute(&pool)
        .await
        .expect("manual edit");
    snap.revision = "three".into();
    snap.records[0] = case("New source edit");
    let b = store.preview_import(&p, "reviewer", &snap).await.expect("conflict");
    assert_eq!(b["preview"]["counts"]["CONFLICT"], 1);
    let result = store
        .apply_import(
            &p,
            "reviewer",
            b["id"].as_str().expect("batch"),
            "three",
            &json!({"testcase:library:one":"keep-local"}),
        )
        .await
        .expect("keep local");
    assert_eq!(result["conflicts"], 0);
    let name: String = sqlx::query_scalar("SELECT name FROM ms_functional_case WHERE id=$1")
        .bind(&native)
        .fetch_one(&pool)
        .await
        .expect("name");
    assert_eq!(name, "Human edit");
    snap.revision = "four".into();
    snap.records[0] = case("Another source edit");
    let b = store.preview_import(&p, "reviewer", &snap).await.expect("still protected");
    assert_eq!(b["preview"]["counts"]["CONFLICT"], 1);
    store
        .apply_import(
            &p,
            "reviewer",
            b["id"].as_str().expect("batch"),
            "four",
            &json!({"testcase:library:one":"use-source"}),
        )
        .await
        .expect("confirmed source");
    snap.revision = "five".into();
    snap.records.remove(0);
    let b = store.preview_import(&p, "reviewer", &snap).await.expect("missing");
    assert_eq!(b["preview"]["counts"]["SOURCE_MISSING"], 1);
    store
        .apply_import(&p, "reviewer", b["id"].as_str().expect("batch"), "five", &json!({}))
        .await
        .expect("retain missing");
    let state: bool = sqlx::query_scalar("SELECT NOT deleted FROM ms_functional_case WHERE id=$1")
        .bind(&native)
        .fetch_one(&pool)
        .await
        .expect("retained");
    assert!(state);
    let overview = store.overview(&p).await.expect("overview");
    assert_eq!(overview["cases"][0]["sourceAvailable"], false);
}
#[tokio::test]
#[ignore = "requires isolated SHEPHERD_ASSETS_TEST_DATABASE_URL"]
async fn service_scope_mapping_and_scenario_execution_are_enforced() {
    let (pool, store, p, other) = fixture().await;
    assert!(store.authorize(&p, "stranger", false).await.is_err());
    store.authorize(&p, "admin", true).await.expect("admin");
    let repo = RepositoryBinding {
        id: "repo".into(),
        name: "Orders".into(),
        branch: "main".into(),
        url: "https://example.com/orders".into(),
        commit_sha: "a".repeat(40),
    };
    let service = store
        .save_service(
            &p,
            None,
            &ServiceInput {
                name: "Orders".into(),
                description: String::new(),
                repositories: vec![repo],
            },
        )
        .await
        .expect("service");
    let sid = service["id"].as_str().expect("id");
    let c:String=sqlx::query_scalar("INSERT INTO ms_functional_case(project_id,name,num) VALUES($1,'Order flow',100001) RETURNING id").bind(&p).fetch_one(&pool).await.expect("case");
    let outsider:String=sqlx::query_scalar("INSERT INTO ms_functional_case(project_id,name,num) VALUES($1,'Other',100001) RETURNING id").bind(&other).fetch_one(&pool).await.expect("outside case");
    assert!(store.assign_cases(&p, Some(sid), &[outsider.clone()]).await.is_err());
    store.assign_cases(&p, Some(sid), &[c.clone()]).await.expect("assign");
    let definition:String=sqlx::query_scalar("INSERT INTO ms_api_definition(project_id,name,method,path) VALUES($1,'Orders','POST','/orders') RETURNING id").bind(&p).fetch_one(&pool).await.expect("API definition");
    let api_case:String=sqlx::query_scalar("INSERT INTO ms_api_case(project_id,api_definition_id,name,method,url) VALUES($1,$2,'Empty order','POST','/orders') RETURNING id").bind(&p).bind(&definition).fetch_one(&pool).await.expect("API case");
    store
        .set_case_api(&p, &c, &[json!({"apiDefinitionId":definition,"apiCaseId":api_case})])
        .await
        .expect("link APIs");
    assert!(store
        .set_case_api(
            &other,
            &outsider,
            &[json!({"apiDefinitionId":definition,"apiCaseId":api_case})]
        )
        .await
        .is_err());
    assert_eq!(
        store.overview(&p).await.expect("links")["apiLinks"].as_array().expect("array").len(),
        1
    );

    assert!(store.delete_service(&p, sid).await.is_err());
    let mut scenario = ScenarioInput {
        name: "Place order".into(),
        description: String::new(),
        preconditions: "Signed in".into(),
        steps: vec![ScenarioStep {
            case_id: outsider,
            instruction: "Submit".into(),
            expected: "Accepted".into(),
            parameters: String::new(),
        }],
        requirement_ids: vec![],
        api_scenario_id: None,
        status: "REVIEWED".into(),
    };
    assert!(store.save_scenario(&p, "reviewer", None, &scenario).await.is_err());
    scenario.steps[0].case_id = c;
    let s = store.save_scenario(&p, "reviewer", None, &scenario).await.expect("scenario");
    let id = s["id"].as_str().expect("id");
    assert!(store.record_run(&p,"reviewer",id,&json!({"revision":99,"environment":"QA v1","steps":[{"outcome":"PASSED","actual":"Accepted"}]})).await.is_err());
    assert!(store
        .record_run(
            &p,
            "reviewer",
            id,
            &json!({"revision":1,"environment":"QA v1","steps":[{"outcome":"PASSED","actual":""}]})
        )
        .await
        .is_err());
    let run=store.record_run(&p,"reviewer",id,&json!({"revision":1,"environment":"QA v1","steps":[{"outcome":"FAILED","actual":"Rejected unexpectedly"}]})).await.expect("run");
    assert_eq!(run["outcome"], "FAILED");
    store.save_scenario(&p, "reviewer", Some(id), &scenario).await.expect("new revision");
    sqlx::query("UPDATE ms_functional_case SET updated_at=now() WHERE id=$1")
        .bind(&scenario.steps[0].case_id)
        .execute(&pool)
        .await
        .expect("changed definition");
    assert!(store.overview(&p).await.expect("overview")["scenarios"][0]["stale"] == true);
    assert!(store.record_run(&p,"reviewer",id,&json!({"revision":2,"environment":"QA v2","steps":[{"outcome":"PASSED","actual":"Accepted"}]})).await.is_err());
    let runs = store.scenario_runs(&p, id).await.expect("history");
    assert_eq!(runs["items"][0]["revision"], 1);
    let inv = json!({"repositoryId":"repo","branch":"main","commitSha":"a".repeat(40),"state":"REVIEWED","endpoints":[{"method":"POST","path":"/orders","file":"src/orders.rs","line":12}],"limitations":[],"scannedPaths":["src/orders.rs"]});
    store.save_inventory(&p, "reviewer", sid, &inv).await.expect("inventory");
    assert!(store.save_inventory(&other, "reviewer", sid, &inv).await.is_err());
}

#[tokio::test]
#[ignore = "requires isolated SHEPHERD_ASSETS_TEST_DATABASE_URL"]
async fn legacy_adoption_and_concurrent_native_edits_preserve_identity() {
    let (pool, store, p, _) = fixture().await;
    sqlx::query("ALTER TABLE ms_functional_case ADD COLUMN IF NOT EXISTS external_source_id TEXT")
        .execute(&pool)
        .await
        .expect("legacy source column");
    sqlx::query("ALTER TABLE ms_functional_case ADD COLUMN IF NOT EXISTS external_id TEXT")
        .execute(&pool)
        .await
        .expect("legacy external column");
    let mut record = case("Order accepted");
    record.data["serial"] = json!("CASE-1");
    let native:String=sqlx::query_scalar("INSERT INTO ms_functional_case(project_id,num,name,priority,steps,custom_fields,external_source_id,external_id) VALUES($1,100001,'CASE-1 Order accepted','P1',$2,'{\"前置条件\":\"Signed in\",\"manual\":\"retained\"}','source','library:one') RETURNING id").bind(&p).bind(&record.data["steps"]).fetch_one(&pool).await.expect("legacy case");
    let missing:String=sqlx::query_scalar("INSERT INTO ms_functional_case(project_id,num,name,external_source_id,external_id) VALUES($1,100002,'Source removed','source','removed-case') RETURNING id").bind(&p).fetch_one(&pool).await.expect("legacy missing");
    let mut snapshot = SourceSnapshot {
        source_id: "source".into(),
        revision: "one".into(),
        records: vec![record],
    };
    let batch = store.preview_import(&p, "reviewer", &snapshot).await.expect("preview");
    assert_eq!(batch["preview"]["counts"]["ADOPT"], 1);
    store
        .apply_import(&p, "reviewer", batch["id"].as_str().expect("id"), "one", &json!({}))
        .await
        .expect("adopt");
    let id: String =
        sqlx::query_scalar("SELECT native_id FROM ms_test_asset_origin WHERE project_id=$1 AND external_id='library:one'")
            .bind(&p)
            .fetch_one(&pool)
            .await
            .expect("origin");
    assert_eq!(id, native);
    let present: bool =
        sqlx::query_scalar("SELECT source_present FROM ms_test_asset_origin WHERE native_id=$1")
            .bind(&missing)
            .fetch_one(&pool)
            .await
            .expect("missing retained with provenance");
    assert!(!present);

    snapshot.revision = "two".into();
    snapshot.records[0] = case("Source changes");
    let batch = store.preview_import(&p, "reviewer", &snapshot).await.expect("preview");
    sqlx::query(
        "UPDATE ms_functional_case SET name='Concurrent human edit',updated_at=now() WHERE id=$1",
    )
    .bind(&native)
    .execute(&pool)
    .await
    .expect("human edit");
    assert!(matches!(
        store
            .apply_import(&p, "reviewer", batch["id"].as_str().expect("id"), "two", &json!({}))
            .await,
        Err(AssetError::Conflict(_))
    ));
    let row: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM ms_functional_case c WHERE id=$1")
        .bind(native)
        .fetch_one(&pool)
        .await
        .expect("preserved");
    assert_eq!(row["name"], "Concurrent human edit");
    assert_eq!(row["custom_fields"]["manual"], "retained");
}
#[tokio::test]
#[ignore = "requires isolated SHEPHERD_ASSETS_TEST_DATABASE_URL"]
async fn http_permissions_and_project_membership_guard_asset_management() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use std::sync::Arc;
    use tower::ServiceExt;
    use webauth::{testing::InMemorySessionStore, PermissionSet, SessionStore};
    let (pool, store, p, _) = fixture().await;
    let sessions = Arc::new(InMemorySessionStore::new());
    let viewer = sessions
        .create(
            "viewer",
            PermissionSet::from_raw(["PROJECT:READ", "FUNCTIONAL_CASE:READ"]).expect("permissions"),
            3600,
        )
        .await
        .expect("session");
    sqlx::query(
        "INSERT INTO ms_project_member(project_id,user_id,role) VALUES($1,'viewer','MEMBER')",
    )
    .bind(&p)
    .execute(&pool)
    .await
    .expect("membership");
    let app = case_management::adapters::test_assets_http::router(
        Arc::new(store),
        None,
        sessions.clone(),
    );
    let path = format!("/test-assets/projects/{p}");
    assert_eq!(
        app.clone()
            .oneshot(Request::builder().uri(&path).body(Body::empty()).expect("request"))
            .await
            .expect("response")
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&path)
                .header("authorization", format!("Bearer {viewer}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let data: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 2_000_000).await.expect("body"),
    )
    .expect("json");
    assert_eq!(data["permissions"]["manage"], false);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{path}/services"))
                .header("authorization", format!("Bearer {viewer}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"Blocked","repositories":[]}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let outsider = sessions
        .create(
            "outsider",
            PermissionSet::from_raw(["PROJECT:READ", "FUNCTIONAL_CASE:READ"]).expect("permissions"),
            3600,
        )
        .await
        .expect("session");
    assert_eq!(
        app.oneshot(
            Request::builder()
                .uri(path)
                .header("authorization", format!("Bearer {outsider}"))
                .body(Body::empty())
                .expect("request")
        )
        .await
        .expect("response")
        .status(),
        StatusCode::FORBIDDEN
    );
}
