use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    routing::patch,
    Json, Router,
};
use serde::Deserialize;
use sqlx::PgPool;
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct RenameState {
    pool: PgPool,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<RenameState> for Arc<dyn SessionStore> {
    fn from_ref(state: &RenameState) -> Self {
        state.sessions.clone()
    }
}

pub fn router(pool: PgPool, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/project/{id}/name", patch(rename))
        .with_state(RenameState { pool, sessions })
}

#[derive(Deserialize)]
struct RenameBody {
    name: String,
}

async fn rename(
    user: AuthUser,
    State(state): State<RenameState>,
    Path(id): Path<String>,
    Json(body): Json<RenameBody>,
) -> Result<StatusCode, StatusCode> {
    if !user.can("PROJECT", "UPDATE") {
        return Err(StatusCode::FORBIDDEN);
    }
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > project::domain::project::MAX_NAME_LEN {
        return Err(StatusCode::BAD_REQUEST);
    }
    let result = sqlx::query(
        "UPDATE ms_project SET name=$2 WHERE id=$1 AND NOT deleted AND
         ($4 OR EXISTS (SELECT 1 FROM ms_project_member WHERE project_id=$1 AND user_id=$3 AND role='OWNER'))",
    )
    .bind(id)
    .bind(name)
    .bind(&user.user_id)
    .bind(user.can("SYSTEM_USER", "UPDATE"))
    .execute(&state.pool)
    .await;
    match result {
        Ok(result) if result.rows_affected() == 1 => Ok(StatusCode::NO_CONTENT),
        Ok(_) => Err(StatusCode::NOT_FOUND),
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(StatusCode::CONFLICT)
        }
        Err(error) => {
            tracing::error!(%error, "Could not rename project");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
