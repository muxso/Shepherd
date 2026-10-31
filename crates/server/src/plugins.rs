//! Host registration for optional integrations. Plugins share sessions, never business storage.
use std::sync::Arc;

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use webauth::{AuthUser, SessionStore};

fn selected(config: &str, id: &str) -> bool {
    config.split(',').any(|value| value.trim() == id)
}

pub fn yunxiao_enabled() -> bool {
    cfg!(feature = "yunxiao")
        && selected(&std::env::var("SHEPHERD_PLUGINS").unwrap_or_default(), "yunxiao")
}

fn catalog(enabled: bool) -> Value {
    let mut plugin: Value =
        serde_json::from_str(include_str!("../../../plugins/yunxiao/plugin.json"))
            .expect("valid bundled plugin manifest");
    plugin["installed"] = json!(cfg!(feature = "yunxiao"));
    plugin["enabled"] = json!(enabled);
    let mut environment: Value =
        serde_json::from_str(include_str!("../../../plugins/environment/plugin.json"))
            .expect("valid environment plugin manifest");
    environment["installed"] = json!(cfg!(feature = "environment-plugins"));
    environment["enabled"] = json!(crate::environment_extension::enabled());
    json!({"items": [plugin, environment]})
}

pub async fn router(
    host_pool: &sqlx::PgPool,
    sessions: Arc<dyn SessionStore>,
) -> Result<Router, Box<dyn std::error::Error>> {
    let enabled = yunxiao_enabled();
    #[cfg(feature = "yunxiao")]
    if enabled {
        let url = std::env::var("SHEPHERD_YUNXIAO_DATABASE_URL")
            .map_err(|_| "enabled Yunxiao plugin requires SHEPHERD_YUNXIAO_DATABASE_URL")?;
        let pool = sqlx::PgPool::connect(&url).await?;
        // Compare the actual database identity, not URL strings or role names.
        let identity =
            "SELECT current_database()::text, inet_server_addr()::text, inet_server_port()";
        type Identity = (String, Option<String>, Option<i32>);
        let host: Identity = sqlx::query_as(identity).fetch_one(host_pool).await?;
        let plugin: Identity = sqlx::query_as(identity).fetch_one(&pool).await?;
        // A distinct name also prevents alternate interfaces/proxies from bypassing isolation.
        if host.0 == plugin.0 {
            return Err("Yunxiao must use a different database from Shepherd".into());
        }
        yunxiao::adapters::pg::migrate(&pool).await?;
        return Ok(registered(pool, sessions, true));
    }
    // No plugin pool is opened or migrated when disabled.
    let _ = (host_pool, enabled);
    Ok(Router::new()
        .route("/api/plugins", get(|_user: AuthUser| async { Json(catalog(false)) }))
        .with_state(sessions))
}

#[cfg(any(feature = "yunxiao", test))]
fn registered(pool: sqlx::PgPool, sessions: Arc<dyn SessionStore>, enabled: bool) -> Router {
    let routes = Router::new()
        .route("/api/plugins", get(move |_user: AuthUser| async move { Json(catalog(enabled)) }))
        .with_state(sessions.clone());
    #[cfg(feature = "yunxiao")]
    if enabled {
        return routes.merge(yunxiao::adapters::http::router(
            yunxiao::application::Service(Arc::new(yunxiao::adapters::pg::PgStore(pool))),
            sessions,
        ));
    }
    let _ = pool;
    routes
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[test]
    fn only_explicit_plugin_ids_enable_integrations() {
        assert!(!selected("", "yunxiao"));
        assert!(!selected("other-yunxiao", "yunxiao"));
        assert!(selected("other, yunxiao ", "yunxiao"));
    }

    #[tokio::test]
    async fn disabled_plugin_is_unmounted_and_catalog_requires_login() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/shepherd_unused")
            .expect("lazy pool");
        let sessions = Arc::new(webauth::testing::InMemorySessionStore::new());
        let token = sessions
            .create(
                "admin",
                webauth::PermissionSet::from_raw(["PROJECT:READ"]).expect("permissions"),
                3600,
            )
            .await
            .expect("session");
        let app = registered(pool, sessions, false);
        for (path, auth, status) in [
            ("/api/devops/sources", false, 404),
            ("/api/plugins", false, 401),
            ("/api/plugins", true, 200),
        ] {
            let mut request = Request::builder().uri(path);
            if auth {
                request = request.header("Authorization", format!("Bearer {token}"));
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).expect("request"))
                .await
                .expect("response");
            assert_eq!(response.status().as_u16(), status);
            if auth {
                let body = axum::body::to_bytes(response.into_body(), 4096).await.expect("body");
                let value: Value = serde_json::from_slice(&body).expect("catalog");
                assert_eq!(value["items"][0]["enabled"], false);
            }
        }
    }

    #[cfg(feature = "yunxiao")]
    #[tokio::test]
    async fn enabled_plugin_mounts_authenticated_sync_routes() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/shepherd_unused")
            .expect("lazy pool");
        let sessions = Arc::new(webauth::testing::InMemorySessionStore::new());
        let app = registered(pool, sessions, true);
        let response = app
            .oneshot(
                Request::builder().uri("/api/devops/sources").body(Body::empty()).expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), 401);
        assert_eq!(catalog(true)["items"][0]["enabled"], true);
    }
}
