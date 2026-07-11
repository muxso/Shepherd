use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use crate::application::{
    CreateEnvironmentError, CreateEnvironmentUseCase, DeleteEnvironmentError,
    DeleteEnvironmentUseCase, EnvironmentInput, GetEnvironmentUseCase, ListEnvironmentsUseCase,
    UpdateEnvironmentError, UpdateEnvironmentUseCase,
};
use crate::domain::Environment;
use crate::ports::EnvironmentRepository;

#[derive(Clone)]
struct EnvironmentState {
    create: CreateEnvironmentUseCase,
    list: ListEnvironmentsUseCase,
    get: GetEnvironmentUseCase,
    update: UpdateEnvironmentUseCase,
    delete: DeleteEnvironmentUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<EnvironmentState> for Arc<dyn SessionStore> {
    fn from_ref(s: &EnvironmentState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(repo: Arc<dyn EnvironmentRepository>, sessions: Arc<dyn SessionStore>) -> Router {
    let state = EnvironmentState {
        create: CreateEnvironmentUseCase::new(repo.clone()),
        list: ListEnvironmentsUseCase::new(repo.clone()),
        get: GetEnvironmentUseCase::new(repo.clone()),
        update: UpdateEnvironmentUseCase::new(repo.clone()),
        delete: DeleteEnvironmentUseCase::new(repo),
        sessions,
    };
    Router::new()
        .route("/api/environment", post(create_environment).get(list_environments))
        .route(
            "/api/environment/{id}",
            axum::routing::get(get_environment).put(update_environment).delete(delete_environment),
        )
        .with_state(state)
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct HeaderDto {
    name: String,
    #[serde(default)]
    value: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct EnvironmentResponse {
    id: String,
    project_id: String,
    name: String,
    base_url: String,
    headers: Vec<HeaderDto>,
    variables: BTreeMap<String, String>,
    enabled: bool,
}

impl From<Environment> for EnvironmentResponse {
    fn from(e: Environment) -> Self {
        Self {
            id: e.id,
            project_id: e.project_id,
            name: e.name,
            base_url: e.base_url,
            headers: e.headers.into_iter().map(|(name, value)| HeaderDto { name, value }).collect(),
            variables: e.variables,
            enabled: e.enabled,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct EnvironmentBody {
    project_id: String,
    name: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    headers: Vec<HeaderDto>,
    #[serde(default)]
    variables: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

impl From<EnvironmentBody> for EnvironmentInput {
    fn from(b: EnvironmentBody) -> Self {
        Self {
            project_id: b.project_id,
            name: b.name,
            base_url: b.base_url,
            headers: b.headers.into_iter().map(|h| (h.name, h.value)).collect(),
            variables: b.variables,
            enabled: b.enabled,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct EnvironmentListQuery {
    project_id: String,
}

#[utoipa::path(post, path = "/api/environment", tag = "environment", request_body = EnvironmentBody, responses((status = 201, body = EnvironmentResponse), (status = 400)), security(("bearer" = [])))]
async fn create_environment(
    user: AuthUser,
    State(st): State<EnvironmentState>,
    Json(req): Json<EnvironmentBody>,
) -> Response {
    if !user.can("ENVIRONMENT", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.create.execute(req.into()).await {
        Ok(e) => (StatusCode::CREATED, Json(EnvironmentResponse::from(e))).into_response(),
        Err(CreateEnvironmentError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid environment payload").into_response()
        }
        Err(CreateEnvironmentError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/api/environment", tag = "environment", params(EnvironmentListQuery), responses((status = 200, body = [EnvironmentResponse])), security(("bearer" = [])))]
async fn list_environments(
    user: AuthUser,
    State(st): State<EnvironmentState>,
    Query(q): Query<EnvironmentListQuery>,
) -> Response {
    if !user.can("ENVIRONMENT", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.list.execute(&q.project_id).await {
        Ok(list) => {
            let items: Vec<EnvironmentResponse> =
                list.into_iter().map(EnvironmentResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(get, path = "/api/environment/{id}", tag = "environment", params(("id" = String, Path)), responses((status = 200, body = EnvironmentResponse), (status = 404)), security(("bearer" = [])))]
async fn get_environment(
    user: AuthUser,
    State(st): State<EnvironmentState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("ENVIRONMENT", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.get.execute(&id).await {
        Ok(Some(e)) => (StatusCode::OK, Json(EnvironmentResponse::from(e))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "environment not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(put, path = "/api/environment/{id}", tag = "environment", params(("id" = String, Path)), request_body = EnvironmentBody, responses((status = 200, body = EnvironmentResponse), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn update_environment(
    user: AuthUser,
    State(st): State<EnvironmentState>,
    Path(id): Path<String>,
    Json(req): Json<EnvironmentBody>,
) -> Response {
    if !user.can("ENVIRONMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.update.execute(&id, req.into()).await {
        Ok(e) => (StatusCode::OK, Json(EnvironmentResponse::from(e))).into_response(),
        Err(UpdateEnvironmentError::NotFound) => {
            (StatusCode::NOT_FOUND, "environment not found").into_response()
        }
        Err(UpdateEnvironmentError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid environment payload").into_response()
        }
        Err(UpdateEnvironmentError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(delete, path = "/api/environment/{id}", tag = "environment", params(("id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn delete_environment(
    user: AuthUser,
    State(st): State<EnvironmentState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("ENVIRONMENT", "DELETE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.delete.execute(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(DeleteEnvironmentError::NotFound) => {
            (StatusCode::NOT_FOUND, "environment not found").into_response()
        }
        Err(DeleteEnvironmentError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(create_environment, list_environments, get_environment, update_environment, delete_environment),
    components(schemas(EnvironmentResponse, EnvironmentBody, HeaderDto)),
    tags((name = "environment", description = "环境"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
