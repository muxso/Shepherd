use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::domain::AuthUser;
use crate::ports::{ApiKeyRecord, ApiKeyRepoError, ApiKeyRepository, PasswordHasher, SessionStore};
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
        .route("/system/apikey/mine", post(create_my_apikey).get(list_my_apikeys))
        .route("/system/apikey", post(create_apikey).get(list_apikeys))
        .route("/system/apikey/{id}", delete(revoke_apikey))
        .route("/system/apikey/{id}/enabled", put(set_apikey_enabled))
        .with_state(ApiKeyState { repo, hasher, sessions })
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
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
    /// 过期时刻(epoch 毫秒);null = 永久。
    expires_at: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ApiKeyItem {
    id: String,
    name: String,
    permissions: Vec<String>,
    created_at: i64,
    expires_at: Option<i64>,
    revoked: bool,
}

impl From<ApiKeyRecord> for ApiKeyItem {
    fn from(r: ApiKeyRecord) -> Self {
        Self {
            id: r.id,
            name: r.name,
            permissions: r.permissions,
            created_at: r.created_at_ms,
            expires_at: r.expires_at_ms,
            revoked: r.revoked,
        }
    }
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

async fn mint_and_insert(
    st: &ApiKeyState,
    name: &str,
    perms: &[String],
    owner: &str,
    expires_at_ms: Option<i64>,
) -> Response {
    let (id, secret) = mint_key();
    let hash = st.hasher.hash(&secret);
    // 自动命名跟随铸出的 id,天然不重名。
    let name = if name.is_empty() { format!("key-{}", &id[..8]) } else { name.to_string() };
    match st.repo.insert(&id, &name, &hash, perms, owner, expires_at_ms).await {
        Ok(rec) => (
            StatusCode::CREATED,
            Json(CreatedApiKey {
                key: format!("sak_{}.{secret}", rec.id),
                id: rec.id,
                name: rec.name,
                permissions: rec.permissions,
                created_at: rec.created_at_ms,
                expires_at: rec.expires_at_ms,
            }),
        )
            .into_response(),
        Err(e) => repo_err_response(e),
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
    // 管理端口径不变:无属主、永久有效。
    mint_and_insert(&st, req.name.trim(), &perms, "", None).await
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CreateMyApiKeyBody {
    /// 缺省自动命名 key-<短id>。
    #[serde(default)]
    name: Option<String>,
    /// 有效期秒数;缺省 null = 永久。
    #[serde(default)]
    ttl_secs: Option<i64>,
}

// 个人自助(act-as-user):仅要求登录,权限 = 会话当前权限快照,属主 = 会话用户。
#[utoipa::path(post, path = "/system/apikey/mine", tag = "apikey", request_body = CreateMyApiKeyBody, responses((status = 201, body = CreatedApiKey), (status = 400), (status = 409)), security(("bearer" = [])))]
async fn create_my_apikey(
    user: AuthUser,
    State(st): State<ApiKeyState>,
    Json(req): Json<CreateMyApiKeyBody>,
) -> Response {
    if req.ttl_secs.is_some_and(|t| t <= 0) {
        return (StatusCode::BAD_REQUEST, "ttlSecs must be positive").into_response();
    }
    let name = req.name.as_deref().unwrap_or("").trim().to_string();
    let perms = user.permissions.to_raw();
    let expires = req.ttl_secs.map(|t| now_ms() + t * 1000);
    mint_and_insert(&st, &name, &perms, &user.user_id, expires).await
}

#[utoipa::path(get, path = "/system/apikey/mine", tag = "apikey", responses((status = 200, body = ApiKeyList)), security(("bearer" = [])))]
async fn list_my_apikeys(user: AuthUser, State(st): State<ApiKeyState>) -> Response {
    match st.repo.list_by_user(&user.user_id).await {
        Ok(recs) => {
            let items = recs.into_iter().map(ApiKeyItem::from).collect();
            (StatusCode::OK, Json(ApiKeyList { items })).into_response()
        }
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
            let items = recs.into_iter().map(ApiKeyItem::from).collect();
            (StatusCode::OK, Json(ApiKeyList { items })).into_response()
        }
        Err(e) => repo_err_response(e),
    }
}

/// 属主本人放行;否则要求 APIKEY:DELETE。不存在 → 404,存在但无权 → 403。
async fn owner_or_delete_perm(user: &AuthUser, st: &ApiKeyState, id: &str) -> Option<Response> {
    if user.can("APIKEY", "DELETE") {
        return None;
    }
    match st.repo.find(id).await {
        Ok(Some(rec)) if rec.user_id == user.user_id => None,
        Ok(Some(_)) => Some((StatusCode::FORBIDDEN, "permission denied").into_response()),
        Ok(None) => Some((StatusCode::NOT_FOUND, "apikey not found").into_response()),
        Err(e) => Some(repo_err_response(e)),
    }
}

#[derive(Deserialize, ToSchema)]
struct SetEnabledBody {
    enabled: bool,
}

// enabled=false 等价吊销,true 恢复;幂等。
#[utoipa::path(put, path = "/system/apikey/{id}/enabled", tag = "apikey", params(("id" = String, Path)), request_body = SetEnabledBody, responses((status = 204), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn set_apikey_enabled(
    user: AuthUser,
    State(st): State<ApiKeyState>,
    Path(id): Path<String>,
    Json(req): Json<SetEnabledBody>,
) -> Response {
    if let Some(denied) = owner_or_delete_perm(&user, &st, &id).await {
        return denied;
    }
    match st.repo.set_enabled(&id, req.enabled).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "apikey not found").into_response(),
        Err(e) => repo_err_response(e),
    }
}

#[utoipa::path(delete, path = "/system/apikey/{id}", tag = "apikey", params(("id" = String, Path)), responses((status = 204), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn revoke_apikey(
    user: AuthUser,
    State(st): State<ApiKeyState>,
    Path(id): Path<String>,
) -> Response {
    if let Some(denied) = owner_or_delete_perm(&user, &st, &id).await {
        return denied;
    }
    match st.repo.revoke(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "apikey not found").into_response(),
        Err(e) => repo_err_response(e),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(create_apikey, list_apikeys, revoke_apikey, create_my_apikey, list_my_apikeys, set_apikey_enabled),
    components(schemas(CreateApiKeyBody, CreateMyApiKeyBody, CreatedApiKey, ApiKeyItem, ApiKeyList, SetEnabledBody)),
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

    struct Ctx {
        app: Router,
        store: Arc<InMemoryApiKeyRepository>,
        sessions: Arc<InMemorySessionStore>,
        admin: String,
        viewer: String,
    }

    async fn ctx() -> Ctx {
        let sessions = Arc::new(InMemorySessionStore::new());
        let store = Arc::new(InMemoryApiKeyRepository::new());
        let app = router(store.clone(), Arc::new(PlainPasswordHasher), sessions.clone());
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
        Ctx { app, store, sessions, admin, viewer }
    }

    async fn app() -> (Router, String, String) {
        let c = ctx().await;
        (c.app, c.admin, c.viewer)
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
        assert!(v["expiresAt"].is_null());

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
        for (method, uri, body) in
            [("POST", "/system/apikey", CREATE), ("GET", "/system/apikey", "")]
        {
            let r = app.clone().oneshot(req(method, uri, body, Some(&viewer))).await.expect("r");
            assert_eq!(r.status(), StatusCode::FORBIDDEN, "{method} {uri}");
        }
        for (method, uri, body) in [
            ("POST", "/system/apikey", CREATE),
            ("GET", "/system/apikey", ""),
            ("DELETE", "/system/apikey/xyz", ""),
            ("POST", "/system/apikey/mine", "{}"),
            ("GET", "/system/apikey/mine", ""),
            ("PUT", "/system/apikey/xyz/enabled", r#"{"enabled":false}"#),
        ] {
            let anon = app.clone().oneshot(req(method, uri, body, None)).await.expect("r");
            assert_eq!(anon.status(), StatusCode::UNAUTHORIZED, "{method} {uri}");
        }
    }

    // —— 个人 API KEY:建 → 列 → 停 → 启 → 过期拒绝 ——

    #[tokio::test]
    async fn my_key_lifecycle_create_list_disable_enable() {
        let c = ctx().await;
        // 无 APIKEY 权限的普通用户也能自助建 key;权限 = 会话快照
        let r = c
            .app
            .clone()
            .oneshot(req("POST", "/system/apikey/mine", "{}", Some(&c.viewer)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = json_body(r).await;
        let id = v["id"].as_str().expect("id").to_string();
        assert_eq!(v["name"], format!("key-{}", &id[..8])); // 自动命名
        assert_eq!(v["permissions"], serde_json::json!(["PROJECT:READ"]));
        assert!(v["expiresAt"].is_null());

        // 只列自己的
        let mine = c
            .app
            .clone()
            .oneshot(req("GET", "/system/apikey/mine", "", Some(&c.viewer)))
            .await
            .expect("r");
        let v = json_body(mine).await;
        assert_eq!(v["items"].as_array().expect("arr").len(), 1);
        assert_eq!(v["items"][0]["revoked"], false);
        let other = c
            .app
            .clone()
            .oneshot(req("GET", "/system/apikey/mine", "", Some(&c.admin)))
            .await
            .expect("r");
        assert!(json_body(other).await["items"].as_array().expect("arr").is_empty());

        // 属主停用 → revoked;再启用 → 恢复
        for (enabled, expect_revoked) in [(false, true), (true, false)] {
            let r = c
                .app
                .clone()
                .oneshot(req(
                    "PUT",
                    &format!("/system/apikey/{id}/enabled"),
                    &format!(r#"{{"enabled":{enabled}}}"#),
                    Some(&c.viewer),
                ))
                .await
                .expect("r");
            assert_eq!(r.status(), StatusCode::NO_CONTENT);
            let rec = c.store.find(&id).await.expect("ok").expect("some");
            assert_eq!(rec.revoked, expect_revoked);
        }

        // 属主可 DELETE 自己的 key(无 APIKEY:DELETE)
        let del = c
            .app
            .clone()
            .oneshot(req("DELETE", &format!("/system/apikey/{id}"), "", Some(&c.viewer)))
            .await
            .expect("r");
        assert_eq!(del.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn my_key_ttl_sets_expiry_and_expired_key_is_rejected() {
        let c = ctx().await;
        let before = now_ms();
        let r = c
            .app
            .clone()
            .oneshot(req(
                "POST",
                "/system/apikey/mine",
                r#"{"name":"short-lived","ttlSecs":60}"#,
                Some(&c.viewer),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = json_body(r).await;
        let exp = v["expiresAt"].as_i64().expect("expiresAt");
        assert!(exp >= before + 60_000 && exp <= now_ms() + 60_000, "{exp}");

        // ttlSecs 非正 → 400
        let bad = c
            .app
            .clone()
            .oneshot(req("POST", "/system/apikey/mine", r#"{"ttlSecs":0}"#, Some(&c.viewer)))
            .await
            .expect("r");
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

        // 校验链路拒绝过期 key:预置一把已过期的,find_active 必须为 None
        c.store
            .insert("aaaa000000000009", "dead", "h", &[], "u-view", Some(1))
            .await
            .expect("seed");
        assert!(c.store.find_active("aaaa000000000009").await.expect("ok").is_none());
    }

    #[tokio::test]
    async fn foreign_key_enable_and_delete_are_denied_without_perm() {
        let c = ctx().await;
        // admin 名下的 key
        let r = c
            .app
            .clone()
            .oneshot(req("POST", "/system/apikey/mine", "{}", Some(&c.admin)))
            .await
            .expect("r");
        let id = json_body(r).await["id"].as_str().expect("id").to_string();

        // viewer 不是属主、无 APIKEY:DELETE → 403;不存在 → 404
        let deny = c
            .app
            .clone()
            .oneshot(req(
                "PUT",
                &format!("/system/apikey/{id}/enabled"),
                r#"{"enabled":false}"#,
                Some(&c.viewer),
            ))
            .await
            .expect("r");
        assert_eq!(deny.status(), StatusCode::FORBIDDEN);
        let deny = c
            .app
            .clone()
            .oneshot(req("DELETE", &format!("/system/apikey/{id}"), "", Some(&c.viewer)))
            .await
            .expect("r");
        assert_eq!(deny.status(), StatusCode::FORBIDDEN);
        let ghost = c
            .app
            .clone()
            .oneshot(req(
                "PUT",
                "/system/apikey/no-such-id/enabled",
                r#"{"enabled":true}"#,
                Some(&c.viewer),
            ))
            .await
            .expect("r");
        assert_eq!(ghost.status(), StatusCode::NOT_FOUND);

        // 有 APIKEY:DELETE 的管理员可停用别人的 key
        let sess = c.sessions.clone();
        let ops = sess
            .create("u-ops", PermissionSet::from_raw(["APIKEY:DELETE"]).expect("p"), 3600)
            .await
            .expect("s");
        let r = c
            .app
            .clone()
            .oneshot(req(
                "PUT",
                &format!("/system/apikey/{id}/enabled"),
                r#"{"enabled":false}"#,
                Some(&ops),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::NO_CONTENT);
    }
}
