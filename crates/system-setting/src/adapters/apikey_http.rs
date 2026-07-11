use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::domain::AuthUser;
use crate::ports::{ApiKeyRepoError, ApiKeyRepository, PasswordHasher, SessionStore};
use kernel::permission::PermissionSet;

#[derive(Clone)]
struct ApiKeyState {
    repo: Arc<dyn ApiKeyRepository>,
    hasher: Arc<dyn PasswordHasher>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<ApiKeyState> for Arc<dyn SessionStore> {
    fn from_ref(s: &ApiKeyState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    repo: Arc<dyn ApiKeyRepository>,
    hasher: Arc<dyn PasswordHasher>,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/system/apikey", post(create_apikey).get(list_apikeys))
        .route("/system/apikey/{id}", delete(revoke_apikey))
        .with_state(ApiKeyState { repo, hasher, sessions })
}

/// id 16 hex + secret 32 hex,均由 v4 UUID(CSPRNG)取得。
fn mint_key() -> (String, String) {
    let id = uuid::Uuid::new_v4().simple().to_string()[..16].to_string();
    let secret = uuid::Uuid::new_v4().simple().to_string();
    (id, secret)
}

#[derive(Deserialize, ToSchema)]
struct CreateApiKeyBody {
    name: String,
    permissions: Vec<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CreatedApiKey {
    id: String,
    name: String,
    /// 完整令牌 `sak_<id>.<secret>`,仅此响应返回一次,之后不可再取。
    key: String,
    permissions: Vec<String>,
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ApiKeyItem {
    id: String,
    name: String,
    permissions: Vec<String>,
    created_at: i64,
    revoked: bool,
}

#[derive(Serialize, ToSchema)]
struct ApiKeyList {
    items: Vec<ApiKeyItem>,
}

fn repo_err_response(e: ApiKeyRepoError) -> Response {
    match e {
        ApiKeyRepoError::NameExists => {
            (StatusCode::CONFLICT, "name already exists").into_response()
        }
        ApiKeyRepoError::Backend(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(post, path = "/system/apikey", tag = "apikey", request_body = CreateApiKeyBody, responses((status = 201, body = CreatedApiKey), (status = 400), (status = 409)), security(("bearer" = [])))]
async fn create_apikey(
    user: AuthUser,
    State(st): State<ApiKeyState>,
    Json(req): Json<CreateApiKeyBody>,
) -> Response {
    if !user.can("APIKEY", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    if req.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "name must not be empty").into_response();
    }
    // 入库前规范化(资源大写、动作排序),保证鉴权路径 from_raw 必然可解析。
    let perms = match PermissionSet::from_raw(&req.permissions) {
        Ok(p) => p.to_raw(),
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid permission").into_response(),
    };
    let (id, secret) = mint_key();
    let hash = st.hasher.hash(&secret);
    match st.repo.insert(&id, req.name.trim(), &hash, &perms).await {
        Ok(rec) => (
            StatusCode::CREATED,
            Json(CreatedApiKey {
                key: format!("sak_{}.{secret}", rec.id),
                id: rec.id,
                name: rec.name,
                permissions: rec.permissions,
                created_at: rec.created_at_ms,
            }),
        )
            .into_response(),
        Err(e) => repo_err_response(e),
    }
}

#[utoipa::path(get, path = "/system/apikey", tag = "apikey", responses((status = 200, body = ApiKeyList)), security(("bearer" = [])))]
async fn list_apikeys(user: AuthUser, State(st): State<ApiKeyState>) -> Response {
    if !user.can("APIKEY", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.repo.list().await {
        Ok(recs) => {
            let items = recs
                .into_iter()
                .map(|r| ApiKeyItem {
                    id: r.id,
                    name: r.name,
                    permissions: r.permissions,
                    created_at: r.created_at_ms,
                    revoked: r.revoked,
                })
                .collect();
            (StatusCode::OK, Json(ApiKeyList { items })).into_response()
        }
        Err(e) => repo_err_response(e),
    }
}

#[utoipa::path(delete, path = "/system/apikey/{id}", tag = "apikey", params(("id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn revoke_apikey(
    user: AuthUser,
    State(st): State<ApiKeyState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("APIKEY", "DELETE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.repo.revoke(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "apikey not found").into_response(),
        Err(e) => repo_err_response(e),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(create_apikey, list_apikeys, revoke_apikey),
    components(schemas(CreateApiKeyBody, CreatedApiKey, ApiKeyItem, ApiKeyList)),
    tags((name = "apikey", description = "API Key(agent 派发凭证)"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{InMemoryApiKeyRepository, InMemorySessionStore, PlainPasswordHasher};
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    async fn app() -> (Router, String, String) {
        let sessions = Arc::new(InMemorySessionStore::new());
        let app = router(
            Arc::new(InMemoryApiKeyRepository::new()),
            Arc::new(PlainPasswordHasher),
            sessions.clone(),
        );
        let admin = sessions
            .create(
                "u-admin",
                PermissionSet::from_raw(["APIKEY:READ+ADD+DELETE"]).expect("p"),
                3600,
            )
            .await
            .expect("s");
        let viewer = sessions
            .create("u-view", PermissionSet::from_raw(["PROJECT:READ"]).expect("p"), 3600)
            .await
            .expect("s");
        (app, admin, viewer)
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

    const CREATE: &str = r#"{"name":"runner-1","permissions":["DELIVERY:READ+UPDATE"]}"#;

    #[tokio::test]
    async fn create_returns_full_key_once_and_list_hides_it() {
        let (app, admin, _) = app().await;
        let r = app
            .clone()
            .oneshot(req("POST", "/system/apikey", CREATE, Some(&admin)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = json_body(r).await;
        let key = v["key"].as_str().expect("key");
        let id = v["id"].as_str().expect("id");
        assert!(key.starts_with(&format!("sak_{id}.")), "{key}");
        assert_eq!(id.len(), 16);
        assert_eq!(key.len(), 4 + 16 + 1 + 32);
        assert_eq!(v["permissions"], serde_json::json!(["DELIVERY:READ+UPDATE"]));
        assert!(v["createdAt"].is_i64());

        let r = app.oneshot(req("GET", "/system/apikey", "", Some(&admin))).await.expect("r");
        assert_eq!(r.status(), StatusCode::OK);
        let v = json_body(r).await;
        let item = &v["items"][0];
        assert_eq!(item["name"], "runner-1");
        assert_eq!(item["revoked"], false);
        // 列表绝不带 key/secret
        assert!(item.get("key").is_none() && item.get("secretHash").is_none(), "{item}");
    }

    #[tokio::test]
    async fn duplicate_name_is_409() {
        let (app, admin, _) = app().await;
        let first = app
            .clone()
            .oneshot(req("POST", "/system/apikey", CREATE, Some(&admin)))
            .await
            .expect("r");
        assert_eq!(first.status(), StatusCode::CREATED);
        let dup =
            app.oneshot(req("POST", "/system/apikey", CREATE, Some(&admin))).await.expect("r");
        assert_eq!(dup.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn invalid_permission_or_empty_name_is_400() {
        let (app, admin, _) = app().await;
        let bad_perm = r#"{"name":"x","permissions":["NO_ACTION"]}"#;
        let r = app
            .clone()
            .oneshot(req("POST", "/system/apikey", bad_perm, Some(&admin)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        let empty_name = r#"{"name":"  ","permissions":[]}"#;
        let r =
            app.oneshot(req("POST", "/system/apikey", empty_name, Some(&admin))).await.expect("r");
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn revoke_then_404_on_repeat_and_unknown() {
        let (app, admin, _) = app().await;
        let r = app
            .clone()
            .oneshot(req("POST", "/system/apikey", CREATE, Some(&admin)))
            .await
            .expect("r");
        let id = json_body(r).await["id"].as_str().expect("id").to_string();
        let del = app
            .clone()
            .oneshot(req("DELETE", &format!("/system/apikey/{id}"), "", Some(&admin)))
            .await
            .expect("r");
        assert_eq!(del.status(), StatusCode::NO_CONTENT);
        // 吊销后记录仍在列表(revoked=true),但再次 DELETE 是 404
        let again = app
            .clone()
            .oneshot(req("DELETE", &format!("/system/apikey/{id}"), "", Some(&admin)))
            .await
            .expect("r");
        assert_eq!(again.status(), StatusCode::NOT_FOUND);
        let ghost = app
            .clone()
            .oneshot(req("DELETE", "/system/apikey/no-such-id", "", Some(&admin)))
            .await
            .expect("r");
        assert_eq!(ghost.status(), StatusCode::NOT_FOUND);
        let list = app.oneshot(req("GET", "/system/apikey", "", Some(&admin))).await.expect("r");
        assert_eq!(json_body(list).await["items"][0]["revoked"], true);
    }

    #[tokio::test]
    async fn guards_403_without_apikey_permission_and_401_anonymous() {
        let (app, _, viewer) = app().await;
        for (method, uri, body) in [
            ("POST", "/system/apikey", CREATE),
            ("GET", "/system/apikey", ""),
            ("DELETE", "/system/apikey/xyz", ""),
        ] {
            let r = app.clone().oneshot(req(method, uri, body, Some(&viewer))).await.expect("r");
            assert_eq!(r.status(), StatusCode::FORBIDDEN, "{method} {uri}");
            let anon = app.clone().oneshot(req(method, uri, body, None)).await.expect("r");
            assert_eq!(anon.status(), StatusCode::UNAUTHORIZED, "{method} {uri}");
        }
    }
}
