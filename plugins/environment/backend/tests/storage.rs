#![cfg(feature = "pg")]
use environment_plugins::{
    domain::*,
    pg::{DatabaseIdentity, PgProfileStore},
    ports::ProfileStore,
};
use plugin_runtime::{PluginConfig, SecretRef};
use serde_json::json;

fn new_profile(name: &str) -> NewProfile {
    NewProfile {
        plugin_id: "redis".into(),
        name: name.into(),
        enabled: true,
        config: PluginConfig {
            schema_version: 1,
            values: [("host".into(), json!("localhost"))].into(),
            secrets: [("password".into(), SecretRef("secret/reference-only".into()))].into(),
        },
    }
}

#[test]
fn profile_and_binding_validation_rejects_unscoped_or_unversioned_configuration() {
    assert!(new_profile("name").validate("").is_err());
    let mut profile = new_profile("name");
    profile.config.schema_version = 0;
    assert!(profile.validate("project").is_err());
    let binding = Binding {
        project_id: "p".into(),
        environment_id: String::new(),
        profile_id: "id".into(),
        slot: BindingSlot::Connection,
        priority: 0,
        enabled: true,
    };
    assert!(binding.validate().is_err());
}

#[tokio::test]
#[ignore = "requires disposable ENVIRONMENT_PLUGIN_TEST_DATABASE_URL and ENVIRONMENT_PLUGIN_TEST_HOST_URL"]
async fn isolated_profiles_persist_with_cas_and_cross_project_binding_protection() {
    let host_url = std::env::var("ENVIRONMENT_PLUGIN_TEST_HOST_URL").expect("disposable host db");
    let plugin_url =
        std::env::var("ENVIRONMENT_PLUGIN_TEST_DATABASE_URL").expect("disposable plugin db");
    let host = sqlx::PgPool::connect(&host_url).await.expect("host connection");
    let pool = sqlx::PgPool::connect(&plugin_url).await.expect("plugin connection");
    let same = DatabaseIdentity::read(&pool).await.expect("plugin identity");
    assert!(matches!(
        PgProfileStore::connect(&plugin_url, &same).await,
        Err(StoreError::HostDatabase)
    ));
    let has_profile: bool = sqlx::query_scalar("SELECT to_regclass('ep_profile') IS NOT NULL")
        .fetch_one(&pool)
        .await
        .expect("table check");
    assert!(!has_profile, "wrong-database guard must run before migrations");
    let identity = DatabaseIdentity::read(&host).await.expect("host identity");
    let store = PgProfileStore::connect(&plugin_url, &identity).await.expect("migrate plugin");
    let one = store.create("project-a", new_profile("first")).await.expect("create");
    assert_eq!(one.revision, 1);
    assert_eq!(one.config.secrets["password"].0, "secret/reference-only");
    assert!(matches!(store.get("project-b", &one.id).await, Err(StoreError::NotFound)));
    assert!(matches!(store.delete("project-b", &one.id, 1).await, Err(StoreError::NotFound)));
    let (a, b) = tokio::join!(
        store.update("project-a", &one.id, 1, new_profile("winner-a")),
        store.update("project-a", &one.id, 1, new_profile("winner-b"))
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(matches!(if a.is_err() { a } else { b }, Err(StoreError::Conflict)));
    let current = store.get("project-a", &one.id).await.expect("current");
    assert_eq!(current.revision, 2);
    let two = store.create("project-a", new_profile("second")).await.expect("second");
    let make_binding = |project: &str, id: &str, priority| Binding {
        project_id: project.into(),
        environment_id: "environment-opaque-reference".into(),
        profile_id: id.into(),
        slot: BindingSlot::Connection,
        priority,
        enabled: true,
    };
    assert!(matches!(
        store.bind(make_binding("project-b", &one.id, 0)).await,
        Err(StoreError::NotFound)
    ));
    let foreign=sqlx::query("INSERT INTO ep_binding(project_id,environment_id,profile_id,slot) VALUES('project-b','e',$1,'connection')")
        .bind(&one.id).execute(&pool).await.expect_err("database must also reject cross-project references");
    assert_eq!(foreign.as_database_error().and_then(|e| e.code()).as_deref(), Some("23503"));
    store.bind(make_binding("project-a", &one.id, 20)).await.expect("bind first");
    store.bind(make_binding("project-a", &two.id, 10)).await.expect("bind second");
    assert!(store
        .bindings("project-b", "environment-opaque-reference")
        .await
        .expect("other project")
        .is_empty());
    let bindings =
        store.bindings("project-a", "environment-opaque-reference").await.expect("bindings");
    assert_eq!(
        bindings.iter().map(|b| b.profile_id.as_str()).collect::<Vec<_>>(),
        vec![two.id.as_str(), one.id.as_str()]
    );
    assert!(matches!(store.delete("project-a", &one.id, 2).await, Err(StoreError::Conflict)));
    let reopened =
        PgProfileStore::connect(&plugin_url, &identity).await.expect("reopen and rerun migrations");
    assert_eq!(reopened.list("project-a").await.expect("persisted profiles").len(), 2);
    assert_eq!(
        reopened
            .bindings("project-a", "environment-opaque-reference")
            .await
            .expect("persisted bindings")
            .len(),
        2
    );
    reopened
        .unbind("project-a", "environment-opaque-reference", BindingSlot::Connection, &one.id)
        .await
        .expect("unbind");
    assert!(matches!(reopened.delete("project-a", &one.id, 1).await, Err(StoreError::Conflict)));
    reopened.delete("project-a", &one.id, 2).await.expect("delete current revision");
    assert!(matches!(reopened.get("project-a", &one.id).await, Err(StoreError::NotFound)));
    let native_tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_tables WHERE schemaname='public' AND tablename LIKE 'ms_%'",
    )
    .fetch_one(&pool)
    .await
    .expect("plugin tables");
    assert_eq!(native_tables, 0);
    let host_tables: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pg_tables WHERE schemaname='public'")
            .fetch_one(&host)
            .await
            .expect("host untouched");
    assert_eq!(host_tables, 0, "the host database must remain untouched");
}

#[test]
fn schema_has_no_host_business_foreign_keys_or_migrations() {
    let migration = include_str!("../migrations/0001_profiles.sql");
    for native in ["ms_environment", "ms_project", "ms_requirement", "ms_functional_case"] {
        assert!(!migration.contains(native));
    }
}
