//! Host registration for optional integrations. Plugins share sessions, never business storage.
use std::sync::Arc;

#[path = "../../../plugins/sonar/backend.rs"]
mod sonar;

use axum::{http::StatusCode, routing::get, Json, Router};
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
    let mut quality: Value =
        serde_json::from_str(include_str!("../../../plugins/sonar/plugin.json"))
            .expect("valid bundled plugin manifest");
    quality["installed"] = json!(true);
    quality["enabled"] =
        json!(selected(&std::env::var("SHEPHERD_PLUGINS").unwrap_or_default(), "sonar"));
    json!({"items": [plugin, environment, quality]})
}

fn managed_catalog(user: AuthUser, enabled: bool) -> Result<Json<Value>, StatusCode> {
    if !user.can("PLUGIN", "READ") {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Json(catalog(enabled)))
}

pub async fn router(
    host_pool: &sqlx::PgPool,
    sessions: Arc<dyn SessionStore>,
    notifier: notice::application::Notifier,
) -> Result<Router, Box<dyn std::error::Error>> {
    let enabled = yunxiao_enabled();
    let sonar_pool = if selected(&std::env::var("SHEPHERD_PLUGINS").unwrap_or_default(), "sonar") {
        Some(sonar::connect(host_pool).await?)
    } else {
        None
    };
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
        let sonar_routes = sonar_pool.as_ref().map(|sonar_pool| {
            sonar::router(
                sonar_pool.clone(),
                sessions.clone(),
                Some(pool.clone()),
                notifier.clone(),
                host_pool.clone(),
            )
        });
        let notification_routes = crate::devops_notification::router(
            host_pool.clone(),
            sessions.clone(),
            Arc::new(
                yunxiao::adapters::pg::PgStore::new(pool.clone())
                    .with_sonar_pool(sonar_pool.clone()),
            ),
        );
        let routes = registered(pool, sessions, true, sonar_pool).merge(notification_routes);
        return Ok(match sonar_routes {
            Some(quality) => routes.merge(quality),
            None => routes,
        });
    }
    // No plugin pool is opened or migrated when disabled.
    let _ = (host_pool, enabled);
    let sonar_routes = sonar_pool
        .map(|pool| sonar::router(pool, sessions.clone(), None, notifier, host_pool.clone()));
    let routes = Router::new()
        .route("/api/plugins", get(|_user: AuthUser| async { Json(catalog(false)) }))
        .route("/api/plugins/manage", get(|user: AuthUser| async { managed_catalog(user, false) }))
        .with_state(sessions);
    Ok(match sonar_routes {
        Some(quality) => routes.merge(quality),
        None => routes,
    })
}

#[cfg(any(feature = "yunxiao", test))]
fn registered(
    pool: sqlx::PgPool,
    sessions: Arc<dyn SessionStore>,
    enabled: bool,
    sonar_pool: Option<sqlx::PgPool>,
) -> Router {
    let routes = Router::new()
        .route("/api/plugins", get(move |_user: AuthUser| async move { Json(catalog(enabled)) }))
        .route(
            "/api/plugins/manage",
            get(move |user: AuthUser| async move { managed_catalog(user, enabled) }),
        )
        .with_state(sessions.clone());
    #[cfg(feature = "yunxiao")]
    if enabled {
        let cache_url = std::env::var("SHEPHERD_YUNXIAO_REDIS_URL")
            .or_else(|_| std::env::var("SHEPHERD_FLEET_REDIS"))
            .or_else(|_| std::env::var("REDIS_URL"))
            .ok();
        let store = yunxiao::adapters::pg::PgStore::new(pool)
            .with_redis_url(cache_url.as_deref())
            .with_sonar_pool(sonar_pool);
        store.start_statistics_warmer();
        return routes.merge(
            yunxiao::adapters::http::router(
                yunxiao::application::Service(Arc::new(store)),
                sessions,
            )
            .layer(tower_http::compression::CompressionLayer::new()),
        );
    }
    let _ = (pool, sonar_pool);
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
        let app = registered(pool, sessions, false, None);
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
        let app = registered(pool, sessions, true, None);
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
