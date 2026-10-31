//! Capability discovery remains available in builds without the environment plugin.
use axum::{http::StatusCode, routing::get, Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;
use webauth::{AuthUser, SessionStore};

pub fn enabled() -> bool {
    false
}
pub struct Runtime;
impl Runtime {
    pub async fn load(_host: &sqlx::PgPool) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        Ok(None)
    }
}
pub fn router(
    _host: sqlx::PgPool,
    sessions: Arc<dyn SessionStore>,
    _runtime: Option<Runtime>,
) -> Router {
    Router::new()
        .route("/api/environment-plugins/capabilities", get(capabilities))
        .with_state(sessions)
}
#[utoipa::path(get,path="/api/environment-plugins/capabilities",tag="environment-plugins",responses((status=200,body=Value),(status=401),(status=403)),security(("bearer"=[])))]
async fn capabilities(user: AuthUser) -> Result<Json<Value>, StatusCode> {
    if !user.can("ENVIRONMENT", "READ") {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Json(json!({"installed":false,"enabled":false,"items":[]})))
}
#[derive(utoipa::OpenApi)]
#[openapi(paths(capabilities))]
struct Api;
pub fn openapi() -> utoipa::openapi::OpenApi {
    <Api as utoipa::OpenApi>::openapi()
}
