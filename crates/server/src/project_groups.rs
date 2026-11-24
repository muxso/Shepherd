use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::Serialize;
use sqlx::PgPool;
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct GroupState {
    pool: PgPool,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<GroupState> for Arc<dyn SessionStore> {
    fn from_ref(state: &GroupState) -> Self {
        state.sessions.clone()
    }
}

pub fn router(pool: PgPool, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/project/{id}/groups", get(list))
        .route("/project/{id}/groups/{roleId}", put(add).delete(remove))
        .with_state(GroupState { pool, sessions })
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct Group {
    id: String,
    name: String,
    scope: String,
    member_count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupList {
    items: Vec<Group>,
    can_manage: bool,
    available: Vec<GroupChoice>,
}

#[derive(Serialize, sqlx::FromRow)]
struct GroupChoice {
    id: String,
    name: String,
}

fn storage(error: sqlx::Error) -> StatusCode {
    tracing::warn!(%error, "project group storage error");
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn can_manage(pool: &PgPool, user: &AuthUser, project: &str) -> Result<bool, StatusCode> {
    if !user.can("PROJECT", "UPDATE") {
        return Ok(false);
    }
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM ms_project p WHERE p.id = $1 AND NOT p.deleted
            AND ($3 OR EXISTS (
                SELECT 1 FROM ms_project_member m
                WHERE m.project_id = p.id AND m.user_id = $2 AND m.role = 'OWNER'
            ))
        )",
    )
    .bind(project)
    .bind(&user.user_id)
    .bind(user.can("SYSTEM_USER", "UPDATE"))
    .fetch_one(pool)
    .await
    .map_err(storage)
}

async fn list(
    user: AuthUser,
    State(state): State<GroupState>,
    Path(project): Path<String>,
) -> Result<Json<GroupList>, StatusCode> {
    if !user.can("PROJECT", "READ") {
        return Err(StatusCode::FORBIDDEN);
    }
    let allowed: bool = sqlx::query_scalar("SELECT ms_has_project_access($1, $2, $3)")
        .bind(&project)
        .bind(&user.user_id)
        .bind(user.can("SYSTEM_USER", "UPDATE"))
        .fetch_one(&state.pool)
        .await
        .map_err(storage)?;
    if !allowed {
        return Err(StatusCode::FORBIDDEN);
    }
    let items = sqlx::query_as(
        "SELECT r.id, r.name, r.scope, count(u.user_id) AS member_count
         FROM ms_project_role g JOIN ms_role r ON r.id = g.role_id
         LEFT JOIN ms_user_role u ON u.role_id = r.id
         WHERE g.project_id = $1 GROUP BY r.id ORDER BY r.name, r.id",
    )
    .bind(&project)
    .fetch_all(&state.pool)
    .await
    .map_err(storage)?;
    let can_manage = can_manage(&state.pool, &user, &project).await?;
    let available = if can_manage {
        sqlx::query_as(
            "SELECT r.id, r.name FROM ms_role r WHERE NOT EXISTS (
                SELECT 1 FROM ms_project_role g WHERE g.project_id = $1 AND g.role_id = r.id
             ) ORDER BY r.name, r.id",
        )
        .bind(&project)
        .fetch_all(&state.pool)
        .await
        .map_err(storage)?
    } else {
        Vec::new()
    };
    Ok(Json(GroupList { items, can_manage, available }))
}

async fn add(
    user: AuthUser,
    State(state): State<GroupState>,
    Path((project, role)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    if !can_manage(&state.pool, &user, &project).await? {
        return Err(StatusCode::FORBIDDEN);
    }
    sqlx::query(
        "INSERT INTO ms_project_role (project_id, role_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(project)
    .bind(role)
    .execute(&state.pool)
    .await
    .map_err(|error| {
        if error.as_database_error().is_some_and(|e| e.is_foreign_key_violation()) {
            StatusCode::NOT_FOUND
        } else {
            storage(error)
        }
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove(
    user: AuthUser,
    State(state): State<GroupState>,
    Path((project, role)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    if !can_manage(&state.pool, &user, &project).await? {
        return Err(StatusCode::FORBIDDEN);
    }
    sqlx::query("DELETE FROM ms_project_role WHERE project_id = $1 AND role_id = $2")
        .bind(project)
        .bind(role)
        .execute(&state.pool)
        .await
        .map_err(storage)?;
    Ok(StatusCode::NO_CONTENT)
}
