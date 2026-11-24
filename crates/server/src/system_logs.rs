use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{FromRef, MatchedPath, Query, Request, State},
    http::{header, Method, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use webauth::{AuthUser, AuthenticatedActor, SessionStore};

#[derive(Clone)]
struct LogState {
    pool: PgPool,
    sessions: Arc<dyn SessionStore>,
    retention_days: i32,
}

impl FromRef<LogState> for Arc<dyn SessionStore> {
    fn from_ref(state: &LogState) -> Self {
        state.sessions.clone()
    }
}

pub fn attach(app: Router, pool: PgPool, sessions: Arc<dyn SessionStore>) -> Router {
    let retention_days = std::env::var("SHEPHERD_SYSTEM_LOG_RETENTION_DAYS")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .map(|days| days.clamp(1, 365))
        .unwrap_or(90);
    tokio::spawn(delete_expired(pool.clone(), retention_days));
    let state = LogState { pool, sessions, retention_days };
    app.merge(Router::new().route("/system/logs", get(list)).with_state(state.clone()))
        .layer(middleware::from_fn_with_state(state, record))
}

async fn delete_expired(pool: PgPool, retention_days: i32) {
    let mut timer = tokio::time::interval(Duration::from_secs(3600));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        timer.tick().await;
        loop {
            let deleted = sqlx::query(
                "DELETE FROM ms_system_operation_log WHERE id IN (
                    SELECT id FROM ms_system_operation_log
                    WHERE recorded_at < now() - make_interval(days => $1)
                    ORDER BY recorded_at LIMIT 1000 FOR UPDATE SKIP LOCKED
                )",
            )
            .bind(retention_days)
            .execute(&pool)
            .await;
            match deleted {
                Ok(result) if result.rows_affected() == 1000 => tokio::task::yield_now().await,
                Ok(_) => break,
                Err(error) => {
                    tracing::warn!(%error, "could not remove expired system operations");
                    break;
                }
            }
        }
    }
}

fn is_audited_view(route: &str) -> bool {
    matches!(
        route,
        "/project/{project}/scripts"
            | "/project/{project}/scripts/{id}/revisions/{revision}"
            | "/requirement/{id}"
            | "/requirement/{id}/version/{n}"
            | "/functional-case"
            | "/functional-case/{id}"
            | "/functional-case/export"
            | "/api/definition/{id}"
            | "/api/scenario/{id}"
            | "/test-plan/{id}"
            | "/skill"
            | "/skill/{id}"
            | "/system/user/{id}"
    )
}

async fn record(State(state): State<LogState>, request: Request, next: Next) -> Response {
    let Some(route) = request.extensions().get::<MatchedPath>().map(MatchedPath::as_str) else {
        return next.run(request).await;
    };
    let method = request.method();
    let login = matches!(
        (method, route),
        (&Method::POST, "/auth/login") | (&Method::GET, "/auth/oidc/{provider}/callback")
    );
    let logout = method == Method::POST && route == "/auth/logout";
    let change = matches!(*method, Method::POST | Method::PUT | Method::PATCH | Method::DELETE)
        && !route.starts_with("/auth/");
    let view = method == Method::GET && is_audited_view(route);
    if !(login || logout || change || view) {
        return next.run(request).await;
    }

    let actor = if login {
        None
    } else {
        let token = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        let Some(token) = token else { return next.run(request).await };
        let Some(session) = state.sessions.get(token).await.ok().flatten() else {
            return next.run(request).await;
        };
        Some(session.user_id)
    };
    let route = route.to_owned();
    let method = method.as_str().to_owned();
    let started = Instant::now();
    let response = next.run(request).await;
    let actor = actor
        .or_else(|| response.extensions().get::<AuthenticatedActor>().map(|actor| actor.0.clone()))
        .unwrap_or_default();
    if let Err(error) = sqlx::query(
        "INSERT INTO ms_system_operation_log (actor_id, method, route, status, duration_ms)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(actor)
    .bind(method)
    .bind(route)
    .bind(i32::from(response.status().as_u16()))
    .bind(started.elapsed().as_millis() as i64)
    .execute(&state.pool)
    .await
    {
        tracing::warn!(%error, "could not record system operation");
    }
    response
}

#[derive(Deserialize)]
struct LogQuery {
    before: Option<i64>,
    status: Option<i32>,
    actor: Option<String>,
    method: Option<String>,
    event: Option<String>,
    route: Option<String>,
    from: Option<chrono::DateTime<chrono::Utc>>,
    until: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct LogItem {
    id: i64,
    recorded_at: chrono::DateTime<chrono::Utc>,
    actor: String,
    actor_id: String,
    method: String,
    event: String,
    route: String,
    status: i32,
    duration_ms: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogPage {
    items: Vec<LogItem>,
    next_before: Option<i64>,
    retention_days: i32,
}

async fn list(
    user: AuthUser,
    State(state): State<LogState>,
    Query(query): Query<LogQuery>,
) -> Result<Json<LogPage>, StatusCode> {
    if !user.can("SYSTEM_LOG", "READ") {
        return Err(StatusCode::FORBIDDEN);
    }
    if query.status.is_some_and(|status| !matches!(status, 2..=5))
        || query
            .method
            .as_deref()
            .is_some_and(|method| !matches!(method, "GET" | "POST" | "PUT" | "PATCH" | "DELETE"))
        || query
            .event
            .as_deref()
            .is_some_and(|event| !matches!(event, "login" | "logout" | "view" | "change"))
        || query.actor.as_ref().is_some_and(|term| term.len() > 128)
        || query.route.as_ref().is_some_and(|term| term.len() > 128)
        || matches!((query.from, query.until), (Some(from), Some(until)) if from >= until)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut items: Vec<LogItem> = sqlx::query_as(
        "SELECT l.id, l.recorded_at, COALESCE(u.name, l.actor_id) AS actor, l.actor_id,
                l.method, event.kind AS event, l.route, l.status, l.duration_ms
         FROM ms_system_operation_log l LEFT JOIN ms_user u ON u.id = l.actor_id
         CROSS JOIN LATERAL (SELECT CASE
            WHEN l.route IN ('/auth/login', '/auth/oidc/{provider}/callback') THEN 'login'
            WHEN l.route = '/auth/logout' THEN 'logout'
            WHEN l.method = 'GET' THEN 'view'
            ELSE 'change'
         END AS kind) event
         WHERE l.recorded_at >= now() - make_interval(days => $1)
           AND ($2::bigint IS NULL OR l.id < $2)
           AND ($3::int IS NULL OR l.status / 100 = $3)
           AND ($4::text IS NULL OR l.method = $4)
           AND ($5::text IS NULL OR strpos(lower(COALESCE(u.name, '')), lower($5)) > 0
                                 OR strpos(lower(l.actor_id), lower($5)) > 0)
           AND ($6::text IS NULL OR strpos(lower(l.route), lower($6)) > 0)
           AND ($7::timestamptz IS NULL OR l.recorded_at >= $7)
           AND ($8::timestamptz IS NULL OR l.recorded_at < $8)
           AND ($9::text IS NULL OR event.kind = $9)
         ORDER BY l.id DESC LIMIT 51",
    )
    .bind(state.retention_days)
    .bind(query.before)
    .bind(query.status)
    .bind(query.method)
    .bind(query.actor)
    .bind(query.route)
    .bind(query.from)
    .bind(query.until)
    .bind(query.event)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::warn!(%error, "could not list system operations");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let next_before = items.get(50).map(|_| items[49].id);
    items.truncate(50);
    Ok(Json(LogPage { items, next_before, retention_days: state.retention_days }))
}
