use std::sync::Arc;

use axum::{
    extract::{FromRef, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use notice::adapters::report_address;
use serde::{Deserialize, Serialize};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct ReportAddressState {
    pool: sqlx::PgPool,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<ReportAddressState> for Arc<dyn SessionStore> {
    fn from_ref(state: &ReportAddressState) -> Self {
        state.sessions.clone()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportAddressView {
    base_url: Option<String>,
    effective_base_url: Option<String>,
    can_edit: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportAddressUpdate {
    base_url: String,
}

pub fn router(pool: sqlx::PgPool, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/system/report-address", get(read).put(write))
        .with_state(ReportAddressState { pool, sessions })
}

async fn read(user: AuthUser, State(state): State<ReportAddressState>) -> Response {
    if !user.can("SYSTEM_PARAMETER", "READ") {
        return StatusCode::FORBIDDEN.into_response();
    }
    match report_address::configured(&state.pool).await {
        Ok(base) => view(base, user.can("SYSTEM_PARAMETER", "UPDATE")),
        Err(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Unable to read report address").into_response()
        }
    }
}

async fn write(
    user: AuthUser,
    State(state): State<ReportAddressState>,
    Json(update): Json<ReportAddressUpdate>,
) -> Response {
    if !user.can("SYSTEM_PARAMETER", "UPDATE") {
        return StatusCode::FORBIDDEN.into_response();
    }
    let origin = match report_address::normalize(&update.base_url) {
        Ok(origin) => origin,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    match report_address::save(&state.pool, &origin).await {
        Ok(()) => view((!origin.is_empty()).then_some(origin), true),
        Err(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Unable to save report address").into_response()
        }
    }
}

fn view(base: Option<String>, can_edit: bool) -> Response {
    Json(ReportAddressView {
        effective_base_url: base.clone().or_else(report_address::deployment_default),
        base_url: base,
        can_edit,
    })
    .into_response()
}
