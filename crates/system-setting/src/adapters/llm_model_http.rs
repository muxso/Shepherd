//! Per-user model settings (/me/llm-model): any logged-in user; CRUD only touches the
//! caller's own configs. api_key is write-only — responses always mask it.

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::domain::AuthUser;
use crate::ports::{
    LlmModelPatch, LlmModelRecord, LlmModelRepoError, LlmModelRepository, SessionStore,
};

#[derive(Clone)]
struct LlmModelState {
    repo: Arc<dyn LlmModelRepository>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<LlmModelState> for Arc<dyn SessionStore> {
    fn from_ref(s: &LlmModelState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(repo: Arc<dyn LlmModelRepository>, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/me/llm-model", get(list_models).post(create_model))
        .route("/me/llm-model/{id}", put(update_model).delete(delete_model))
        .with_state(LlmModelState { repo, sessions })
}

/// Empty string → ""; otherwise "****" + last 4 chars. Never returns the plaintext.
fn mask_api_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let n = key.chars().count();
    let tail: String = key.chars().skip(n.saturating_sub(4)).collect();
    format!("****{tail}")
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct LlmModelItem {
    id: String,
    provider: String,
    name: String,
    base_url: String,
    api_key_masked: String,
    enabled: bool,
    created_at: i64,
}

impl From<LlmModelRecord> for LlmModelItem {
    fn from(r: LlmModelRecord) -> Self {
        Self {
            id: r.id,
            provider: r.provider,
            name: r.name,
            base_url: r.base_url,
            api_key_masked: mask_api_key(&r.api_key),
            enabled: r.enabled,
            created_at: r.created_at_ms,
        }
    }
}

#[derive(Serialize, ToSchema)]
struct LlmModelList {
    items: Vec<LlmModelItem>,
}

fn repo_err_response(e: LlmModelRepoError) -> Response {
    match e {
        LlmModelRepoError::Duplicate => {
            (StatusCode::CONFLICT, "model already exists").into_response()
        }
        LlmModelRepoError::Backend(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

/// Shared rule for provider/name: non-empty after trim and ≤64 chars.
fn valid_field(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty() && t.chars().count() <= 64
}

#[utoipa::path(get, path = "/me/llm-model", tag = "llm-model", responses((status = 200, body = LlmModelList), (status = 401)), security(("bearer" = [])))]
async fn list_models(user: AuthUser, State(st): State<LlmModelState>) -> Response {
    match st.repo.list_by_user(&user.user_id).await {
        Ok(recs) => {
            let items = recs.into_iter().map(LlmModelItem::from).collect();
            (StatusCode::OK, Json(LlmModelList { items })).into_response()
        }
        Err(e) => repo_err_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CreateLlmModelBody {
    provider: String,
    name: String,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
}

#[utoipa::path(post, path = "/me/llm-model", tag = "llm-model", request_body = CreateLlmModelBody, responses((status = 201, body = LlmModelItem), (status = 400), (status = 409)), security(("bearer" = [])))]
async fn create_model(
    user: AuthUser,
    State(st): State<LlmModelState>,
    Json(req): Json<CreateLlmModelBody>,
) -> Response {
    if !valid_field(&req.provider) || !valid_field(&req.name) {
        return (StatusCode::BAD_REQUEST, "provider/name must be 1..=64 chars").into_response();
    }
    let provider = req.provider.trim().to_ascii_lowercase();
    let name = req.name.trim();
    let base_url = req.base_url.as_deref().unwrap_or("");
    let api_key = req.api_key.as_deref().unwrap_or("");
    match st.repo.insert(&user.user_id, &provider, name, base_url, api_key).await {
        Ok(rec) => (StatusCode::CREATED, Json(LlmModelItem::from(rec))).into_response(),
        Err(e) => repo_err_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct UpdateLlmModelBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

#[utoipa::path(put, path = "/me/llm-model/{id}", tag = "llm-model", params(("id" = String, Path)), request_body = UpdateLlmModelBody, responses((status = 200, body = LlmModelItem), (status = 400), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn update_model(
    user: AuthUser,
    State(st): State<LlmModelState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateLlmModelBody>,
) -> Response {
    if req.name.as_deref().is_some_and(|n| !valid_field(n)) {
        return (StatusCode::BAD_REQUEST, "name must be 1..=64 chars").into_response();
    }
    let patch = LlmModelPatch {
        name: req.name.map(|n| n.trim().to_string()),
        base_url: req.base_url,
        api_key: req.api_key,
        enabled: req.enabled,
    };
    match st.repo.update(&user.user_id, &id, patch).await {
        Ok(Some(rec)) => (StatusCode::OK, Json(LlmModelItem::from(rec))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "model not found").into_response(),
        Err(e) => repo_err_response(e),
    }
}

#[utoipa::path(delete, path = "/me/llm-model/{id}", tag = "llm-model", params(("id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn delete_model(
    user: AuthUser,
    State(st): State<LlmModelState>,
    Path(id): Path<String>,
) -> Response {
    match st.repo.delete(&user.user_id, &id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "model not found").into_response(),
        Err(e) => repo_err_response(e),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(list_models, create_model, update_model, delete_model),
    components(schemas(LlmModelItem, LlmModelList, CreateLlmModelBody, UpdateLlmModelBody)),
    tags((name = "llm-model", description = "个人模型设置"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{InMemoryLlmModelRepository, InMemorySessionStore};
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;

    async fn app() -> (Router, String, String) {
        let sessions = Arc::new(InMemorySessionStore::new());
        let app = router(Arc::new(InMemoryLlmModelRepository::new()), sessions.clone());
        let alice = sessions
            .create("u-alice", PermissionSet::from_raw(["PROJECT:READ"]).expect("p"), 3600)
            .await
            .expect("s");
        let bob = sessions
            .create("u-bob", PermissionSet::from_raw(["PROJECT:READ"]).expect("p"), 3600)
            .await
            .expect("s");
        (app, alice, bob)
    }

    fn req(method: &str, uri: &str, body: &str, bearer: Option<&str>) -> Request<Body> {
        let mut b =
            Request::builder().method(method).uri(uri).header("content-type", "application/json");
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    async fn json_body(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    const CREATE: &str = r#"{"provider":"DeepSeek","name":"deepseek-chat","baseUrl":"https://api.deepseek.com","apiKey":"sk-abc12345"}"#;

    #[tokio::test]
    async fn crud_flow_with_masked_api_key() {
        let (app, alice, _) = app().await;
        let r = app
            .clone()
            .oneshot(req("POST", "/me/llm-model", CREATE, Some(&alice)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = json_body(r).await;
        let id = v["id"].as_str().expect("id").to_string();
        assert_eq!(v["provider"], "deepseek"); // lowercased on storage
        assert_eq!(v["name"], "deepseek-chat");
        assert_eq!(v["baseUrl"], "https://api.deepseek.com");
        assert_eq!(v["apiKeyMasked"], "****2345"); // mask: **** + last 4 chars
        assert_eq!(v["enabled"], true);
        assert!(v["createdAt"].is_i64());
        assert!(v.get("apiKey").is_none(), "{v}"); // plaintext never returned

        let list =
            app.clone().oneshot(req("GET", "/me/llm-model", "", Some(&alice))).await.expect("r");
        assert_eq!(list.status(), StatusCode::OK);
        let v = json_body(list).await;
        assert_eq!(v["items"].as_array().expect("arr").len(), 1);
        assert_eq!(v["items"][0]["apiKeyMasked"], "****2345");

        let upd = app
            .clone()
            .oneshot(req(
                "PUT",
                &format!("/me/llm-model/{id}"),
                r#"{"name":"deepseek-reasoner","enabled":false}"#,
                Some(&alice),
            ))
            .await
            .expect("r");
        assert_eq!(upd.status(), StatusCode::OK);
        let v = json_body(upd).await;
        assert_eq!(v["name"], "deepseek-reasoner");
        assert_eq!(v["enabled"], false);
        assert_eq!(v["apiKeyMasked"], "****2345"); // untouched key preserved

        let del = app
            .clone()
            .oneshot(req("DELETE", &format!("/me/llm-model/{id}"), "", Some(&alice)))
            .await
            .expect("r");
        assert_eq!(del.status(), StatusCode::NO_CONTENT);
        let list = app.oneshot(req("GET", "/me/llm-model", "", Some(&alice))).await.expect("r");
        assert!(json_body(list).await["items"].as_array().expect("arr").is_empty());
    }

    #[tokio::test]
    async fn empty_api_key_masks_to_empty_string() {
        let (app, alice, _) = app().await;
        let r = app
            .clone()
            .oneshot(req(
                "POST",
                "/me/llm-model",
                r#"{"provider":"custom","name":"local-llm"}"#,
                Some(&alice),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = json_body(r).await;
        assert_eq!(v["apiKeyMasked"], "");
        assert_eq!(v["baseUrl"], "");
    }

    #[tokio::test]
    async fn duplicate_and_invalid_fields() {
        let (app, alice, bob) = app().await;
        let first = app
            .clone()
            .oneshot(req("POST", "/me/llm-model", CREATE, Some(&alice)))
            .await
            .expect("r");
        assert_eq!(first.status(), StatusCode::CREATED);
        // Same user+provider+name → 409; another user does not conflict.
        let dup = app
            .clone()
            .oneshot(req("POST", "/me/llm-model", CREATE, Some(&alice)))
            .await
            .expect("r");
        assert_eq!(dup.status(), StatusCode::CONFLICT);
        let other =
            app.clone().oneshot(req("POST", "/me/llm-model", CREATE, Some(&bob))).await.expect("r");
        assert_eq!(other.status(), StatusCode::CREATED);

        for body in [
            r#"{"provider":"  ","name":"m"}"#,
            r#"{"provider":"p","name":""}"#,
            &format!(r#"{{"provider":"p","name":"{}"}}"#, "x".repeat(65)),
        ] {
            let r = app
                .clone()
                .oneshot(req("POST", "/me/llm-model", body, Some(&alice)))
                .await
                .expect("r");
            assert_eq!(r.status(), StatusCode::BAD_REQUEST, "{body}");
        }
    }

    #[tokio::test]
    async fn foreign_or_unknown_rows_are_404() {
        let (app, alice, bob) = app().await;
        let r = app
            .clone()
            .oneshot(req("POST", "/me/llm-model", CREATE, Some(&alice)))
            .await
            .expect("r");
        let id = json_body(r).await["id"].as_str().expect("id").to_string();

        // Cross-user access (bob updating/deleting alice's row) and a missing id are
        // both 404.
        let upd = app
            .clone()
            .oneshot(req("PUT", &format!("/me/llm-model/{id}"), r#"{"enabled":false}"#, Some(&bob)))
            .await
            .expect("r");
        assert_eq!(upd.status(), StatusCode::NOT_FOUND);
        let del = app
            .clone()
            .oneshot(req("DELETE", &format!("/me/llm-model/{id}"), "", Some(&bob)))
            .await
            .expect("r");
        assert_eq!(del.status(), StatusCode::NOT_FOUND);
        let ghost = app
            .clone()
            .oneshot(req("PUT", "/me/llm-model/no-such-id", "{}", Some(&alice)))
            .await
            .expect("r");
        assert_eq!(ghost.status(), StatusCode::NOT_FOUND);

        // Anonymous → 401.
        let anon = app.oneshot(req("GET", "/me/llm-model", "", None)).await.expect("r");
        assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rename_collision_is_409() {
        let (app, alice, _) = app().await;
        let a = app
            .clone()
            .oneshot(req("POST", "/me/llm-model", CREATE, Some(&alice)))
            .await
            .expect("r");
        json_body(a).await;
        let b = app
            .clone()
            .oneshot(req(
                "POST",
                "/me/llm-model",
                r#"{"provider":"deepseek","name":"deepseek-reasoner"}"#,
                Some(&alice),
            ))
            .await
            .expect("r");
        let id_b = json_body(b).await["id"].as_str().expect("id").to_string();
        let r = app
            .oneshot(req(
                "PUT",
                &format!("/me/llm-model/{id_b}"),
                r#"{"name":"deepseek-chat"}"#,
                Some(&alice),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CONFLICT);
    }
}
