use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use crate::application::{TemplateCmdError, TemplateService};
use crate::domain::Template;

#[derive(Clone)]
struct TemplateState {
    templates: TemplateService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<TemplateState> for Arc<dyn SessionStore> {
    fn from_ref(s: &TemplateState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(templates: TemplateService, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/project/{id}/template", get(list_templates).post(create_template))
        .route("/template/{id}", put(update_template).delete(delete_template))
        .with_state(TemplateState { templates, sessions })
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct TemplateResponse {
    id: String,
    project_id: String,
    kind: String,
    name: String,
    #[schema(value_type = Object)]
    config: serde_json::Value,
    created_by: String,
    created_at: i64,
    updated_at: i64,
}

impl From<Template> for TemplateResponse {
    fn from(t: Template) -> Self {
        Self {
            id: t.id,
            project_id: t.project_id,
            kind: t.kind,
            name: t.name,
            config: t.config,
            created_by: t.created_by,
            created_at: t.created_at_ms,
            updated_at: t.updated_at_ms,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
struct TemplateListResponse {
    items: Vec<TemplateResponse>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CreateTemplateRequest {
    kind: String,
    name: String,
    #[serde(default = "empty_object")]
    #[schema(value_type = Object)]
    config: serde_json::Value,
}

fn empty_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct UpdateTemplateRequest {
    name: Option<String>,
    #[schema(value_type = Object)]
    config: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, IntoParams)]
struct ListQuery {
    kind: Option<String>,
}

fn cmd_err(e: TemplateCmdError) -> Response {
    match e {
        TemplateCmdError::Validation(_) => {
            (StatusCode::BAD_REQUEST, "invalid template payload").into_response()
        }
        TemplateCmdError::NameExists => {
            (StatusCode::CONFLICT, "template name already exists").into_response()
        }
        TemplateCmdError::NotFound => (StatusCode::NOT_FOUND, "template not found").into_response(),
        TemplateCmdError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(
    post, path = "/project/{id}/template", tag = "template",
    params(("id" = String, Path)),
    request_body = CreateTemplateRequest,
    responses((status = 201, body = TemplateResponse), (status = 400), (status = 403), (status = 409)),
    security(("bearer" = []))
)]
async fn create_template(
    user: AuthUser,
    State(st): State<TemplateState>,
    Path(id): Path<String>,
    Json(req): Json<CreateTemplateRequest>,
) -> Response {
    if !user.can("PROJECT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.templates.create(&id, &req.kind, &req.name, req.config, &user.user_id).await {
        Ok(t) => (StatusCode::CREATED, Json(TemplateResponse::from(t))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(
    get, path = "/project/{id}/template", tag = "template",
    params(("id" = String, Path), ListQuery),
    responses((status = 200, body = TemplateListResponse), (status = 400), (status = 403)),
    security(("bearer" = []))
)]
async fn list_templates(
    user: AuthUser,
    State(st): State<TemplateState>,
    Path(id): Path<String>,
    Query(q): Query<ListQuery>,
) -> Response {
    if !user.can("PROJECT", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.templates.list(&id, q.kind.as_deref()).await {
        Ok(list) => {
            let items: Vec<TemplateResponse> =
                list.into_iter().map(TemplateResponse::from).collect();
            (StatusCode::OK, Json(TemplateListResponse { items })).into_response()
        }
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(
    put, path = "/template/{id}", tag = "template",
    params(("id" = String, Path)),
    request_body = UpdateTemplateRequest,
    responses((status = 200, body = TemplateResponse), (status = 400), (status = 403), (status = 404), (status = 409)),
    security(("bearer" = []))
)]
async fn update_template(
    user: AuthUser,
    State(st): State<TemplateState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTemplateRequest>,
) -> Response {
    if !user.can("PROJECT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.templates.update(&id, req.name.as_deref(), req.config).await {
        Ok(t) => (StatusCode::OK, Json(TemplateResponse::from(t))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(
    delete, path = "/template/{id}", tag = "template",
    params(("id" = String, Path)),
    responses((status = 204), (status = 403), (status = 404)),
    security(("bearer" = []))
)]
async fn delete_template(
    user: AuthUser,
    State(st): State<TemplateState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("PROJECT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.templates.delete(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => cmd_err(e),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(create_template, list_templates, update_template, delete_template),
    components(schemas(
        CreateTemplateRequest,
        UpdateTemplateRequest,
        TemplateResponse,
        TemplateListResponse
    )),
    tags((name = "template", description = "Project template"))
)]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryTemplateRepository;
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app_with(perm: &str) -> (Router, String) {
        let repo = Arc::new(InMemoryTemplateRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw([perm.to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        (router(TemplateService::new(repo), sessions), token)
    }

    fn json_req(method: &str, uri: &str, body: Option<&str>, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method(method).uri(uri);
        if body.is_some() {
            b = b.header("content-type", "application/json");
        }
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(body.map(|s| Body::from(s.to_string())).unwrap_or(Body::empty())).expect("req")
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    #[tokio::test]
    async fn create_then_list_template() {
        let (app, t) = app_with("PROJECT:READ+UPDATE").await;
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/project/p1/template",
                Some(r#"{"kind":"Requirement","name":"default","config":{"fields":[1]}}"#),
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = body_json(resp).await;
        assert_eq!(v["projectId"], "p1");
        assert_eq!(v["kind"], "requirement"); // normalized to lowercase
        assert_eq!(v["name"], "default");
        assert_eq!(v["config"], serde_json::json!({"fields": [1]}));
        assert_eq!(v["createdBy"], "admin");
        assert!(v["createdAt"].as_i64().expect("ms") > 0);
        assert_eq!(v["createdAt"], v["updatedAt"]);

        let resp = app
            .oneshot(json_req("GET", "/project/p1/template?kind=requirement", None, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        let items = v["items"].as_array().expect("items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["name"], "default");
    }

    #[tokio::test]
    async fn create_defaults_config_to_empty_object() {
        let (app, t) = app_with("PROJECT:UPDATE").await;
        let resp = app
            .oneshot(json_req(
                "POST",
                "/project/p1/template",
                Some(r#"{"kind":"bug","name":"B"}"#),
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(body_json(resp).await["config"], serde_json::json!({}));
    }

    #[tokio::test]
    async fn list_filters_by_kind() {
        let (app, t) = app_with("PROJECT:READ+UPDATE").await;
        for body in [r#"{"kind":"requirement","name":"R"}"#, r#"{"kind":"bug","name":"B"}"#] {
            let resp = app
                .clone()
                .oneshot(json_req("POST", "/project/p1/template", Some(body), Some(&t)))
                .await
                .expect("seed");
            assert_eq!(resp.status(), StatusCode::CREATED);
        }
        let resp = app
            .clone()
            .oneshot(json_req("GET", "/project/p1/template", None, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(body_json(resp).await["items"].as_array().expect("items").len(), 2);
        let resp = app
            .oneshot(json_req("GET", "/project/p1/template?kind=bug", None, Some(&t)))
            .await
            .expect("resp");
        let v = body_json(resp).await;
        assert_eq!(v["items"].as_array().expect("items").len(), 1);
        assert_eq!(v["items"][0]["kind"], "bug");
    }

    #[tokio::test]
    async fn create_duplicate_name_409() {
        let (app, t) = app_with("PROJECT:UPDATE").await;
        let body = r#"{"kind":"requirement","name":"default"}"#;
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/project/p1/template", Some(body), Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let resp = app
            .oneshot(json_req("POST", "/project/p1/template", Some(body), Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn create_invalid_payload_400() {
        let (app, t) = app_with("PROJECT:UPDATE").await;
        let resp = app
            .oneshot(json_req(
                "POST",
                "/project/p1/template",
                Some(r#"{"kind":"  ","name":"A"}"#),
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn update_renames_and_reconfigs() {
        let (app, t) = app_with("PROJECT:UPDATE").await;
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/project/p1/template",
                Some(r#"{"kind":"requirement","name":"A"}"#),
                Some(&t),
            ))
            .await
            .expect("seed");
        let id = body_json(resp).await["id"].as_str().expect("id").to_string();
        let resp = app
            .oneshot(json_req(
                "PUT",
                &format!("/template/{id}"),
                Some(r#"{"name":"B","config":{"x":1}}"#),
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["name"], "B");
        assert_eq!(v["config"], serde_json::json!({"x": 1}));
        assert!(v["updatedAt"].as_i64().expect("ms") > v["createdAt"].as_i64().expect("ms"));
    }

    #[tokio::test]
    async fn update_unknown_404_and_duplicate_409() {
        let (app, t) = app_with("PROJECT:UPDATE").await;
        let resp = app
            .clone()
            .oneshot(json_req("PUT", "/template/nope", Some(r#"{"name":"X"}"#), Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.clone()
            .oneshot(json_req(
                "POST",
                "/project/p1/template",
                Some(r#"{"kind":"requirement","name":"A"}"#),
                Some(&t),
            ))
            .await
            .expect("seed A");
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/project/p1/template",
                Some(r#"{"kind":"requirement","name":"B"}"#),
                Some(&t),
            ))
            .await
            .expect("seed B");
        let id = body_json(resp).await["id"].as_str().expect("id").to_string();
        let resp = app
            .oneshot(json_req("PUT", &format!("/template/{id}"), Some(r#"{"name":"A"}"#), Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn delete_then_404() {
        let (app, t) = app_with("PROJECT:UPDATE").await;
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/project/p1/template",
                Some(r#"{"kind":"requirement","name":"A"}"#),
                Some(&t),
            ))
            .await
            .expect("seed");
        let id = body_json(resp).await["id"].as_str().expect("id").to_string();
        let resp = app
            .clone()
            .oneshot(json_req("DELETE", &format!("/template/{id}"), None, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let resp = app
            .oneshot(json_req("DELETE", &format!("/template/{id}"), None, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn write_without_update_permission_403() {
        let (app, t) = app_with("PROJECT:READ").await;
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/project/p1/template",
                Some(r#"{"kind":"requirement","name":"A"}"#),
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let resp = app
            .clone()
            .oneshot(json_req("PUT", "/template/x", Some(r#"{"name":"B"}"#), Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let resp =
            app.oneshot(json_req("DELETE", "/template/x", None, Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn list_without_read_permission_403_and_without_token_401() {
        let (app, t) = app_with("PROJECT:UPDATE").await;
        let resp = app
            .clone()
            .oneshot(json_req("GET", "/project/p1/template", None, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let resp =
            app.oneshot(json_req("GET", "/project/p1/template", None, None)).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
