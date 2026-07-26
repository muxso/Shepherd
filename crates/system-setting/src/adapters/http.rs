use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::{
        header::{AUTHORIZATION, COOKIE, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};

use crate::application::{
    CreateUserError, CreateUserUseCase, LoginUseCase, OidcLoginUseCase, OrgCmdError,
    OrganizationService, ResolveUserNamesUseCase, RoleCmdError, RoleService, UserCmdError,
    UserRoleService, UserService,
};
use crate::domain::RoleScope;
use crate::domain::{AuthError, AuthUser, OidcError};
use crate::ports::{CredentialRepository, PasswordHasher, SessionStore, UserRoleQuery};

#[cfg(all(feature = "http", feature = "oidc"))]
use crate::adapters::oidc::build_provider;
#[cfg(all(feature = "http", feature = "oidc"))]
use crate::domain::{OidcProvider, OidcRepoError};
#[cfg(all(feature = "http", feature = "oidc"))]
use crate::ports::OidcProviderRepository;
use kernel::page::PageRequest;

#[derive(Clone)]
struct AppState {
    create: CreateUserUseCase,
    resolve: ResolveUserNamesUseCase,
    login: LoginUseCase,
    admin: UserService,
    creds: Arc<dyn CredentialRepository>,
    hasher: Arc<dyn PasswordHasher>,
    user_roles: Arc<dyn UserRoleQuery>,
    sessions: Arc<dyn SessionStore>,
    ttl_secs: i64,
}

impl FromRef<AppState> for Arc<dyn SessionStore> {
    fn from_ref(s: &AppState) -> Self {
        s.sessions.clone()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn router(
    create: CreateUserUseCase,
    resolve: ResolveUserNamesUseCase,
    login: LoginUseCase,
    admin: UserService,
    creds: Arc<dyn CredentialRepository>,
    hasher: Arc<dyn PasswordHasher>,
    user_roles: Arc<dyn UserRoleQuery>,
    sessions: Arc<dyn SessionStore>,
    ttl_secs: i64,
) -> Router {
    Router::new()
        .route("/auth/login", post(login_handler))
        .route("/auth/logout", post(logout_handler))
        .route("/auth/refresh", post(refresh_handler))
        .route("/auth/me", get(me_handler))
        .route("/auth/password", post(change_password))
        // The static segment /names must be registered before /{id}, or the wildcard eats it.
        .route("/system/user/names", get(resolve_names))
        .route("/system/user", post(create_user).get(list_users))
        .route("/system/user/{id}", get(get_user).put(update_user).delete(delete_user))
        .route("/system/user/{id}/reset-password", post(reset_password))
        .with_state(AppState {
            create,
            resolve,
            login,
            admin,
            creds,
            hasher,
            user_roles,
            sessions,
            ttl_secs,
        })
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|s| s.strip_prefix("Bearer "))
}

#[derive(Deserialize, ToSchema)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct LoginResponse {
    token: String,
    /// Session user id (the value used by created_by and other audit fields); omitted if
    /// the session lookup fails.
    #[serde(skip_serializing_if = "Option::is_none")]
    user_id: Option<String>,
}

#[utoipa::path(post, path = "/auth/login", tag = "auth", request_body = LoginRequest, responses((status = 200, body = LoginResponse), (status = 401), (status = 429)))]
async fn login_handler(State(st): State<AppState>, Json(req): Json<LoginRequest>) -> Response {
    match st.login.execute(&req.username, &req.password).await {
        Ok(token) => {
            let user_id = st.sessions.get(&token).await.ok().flatten().map(|s| s.user_id);
            (StatusCode::OK, Json(LoginResponse { token, user_id })).into_response()
        }
        Err(AuthError::InvalidCredentials) => {
            (StatusCode::UNAUTHORIZED, "invalid credentials").into_response()
        }
        Err(AuthError::LockedOut) => {
            (StatusCode::TOO_MANY_REQUESTS, "too many failed attempts, try later").into_response()
        }
        Err(AuthError::Backend(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "auth error").into_response()
        }
    }
}

#[utoipa::path(post, path = "/auth/logout", tag = "auth", responses((status = 204)), security(("bearer" = [])))]
async fn logout_handler(State(st): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = bearer(&headers) {
        let _ = st.sessions.revoke(token).await;
    }
    StatusCode::NO_CONTENT.into_response()
}

// Sliding session: exchange the current valid token for a new one (fresh TTL); the old
// token is invalidated immediately.
#[utoipa::path(post, path = "/auth/refresh", tag = "auth", responses((status = 200, body = LoginResponse), (status = 401)), security(("bearer" = [])))]
async fn refresh_handler(State(st): State<AppState>, headers: HeaderMap) -> Response {
    let Some(token) = bearer(&headers) else {
        return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response();
    };
    match st.sessions.get(token).await {
        Ok(Some(session)) => {
            match st.sessions.create(&session.user_id, session.permissions, st.ttl_secs).await {
                Ok(fresh) => {
                    let _ = st.sessions.revoke(token).await;
                    let user_id = Some(session.user_id);
                    (StatusCode::OK, Json(LoginResponse { token: fresh, user_id })).into_response()
                }
                Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "auth error").into_response(),
            }
        }
        Ok(None) => (StatusCode::UNAUTHORIZED, "invalid or expired token").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "auth error").into_response(),
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct MeResponse {
    user_id: String,
    /// Raw permission strings (`RESOURCE:A+B`), same as the login session.
    permissions: Vec<String>,
}

#[utoipa::path(get, path = "/auth/me", tag = "auth", responses((status = 200, body = MeResponse), (status = 401)), security(("bearer" = [])))]
async fn me_handler(user: AuthUser) -> Response {
    let body = MeResponse { permissions: user.permissions.to_raw(), user_id: user.user_id };
    (StatusCode::OK, Json(body)).into_response()
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ChangePasswordBody {
    old_password: String,
    new_password: String,
}

// Self-service password change: the old password goes through the same hash verification
// as login; existing sessions are not revoked afterwards.
#[utoipa::path(post, path = "/auth/password", tag = "auth", request_body = ChangePasswordBody, responses((status = 204), (status = 400), (status = 401), (status = 409)), security(("bearer" = [])))]
async fn change_password(
    user: AuthUser,
    State(st): State<AppState>,
    Json(req): Json<ChangePasswordBody>,
) -> Response {
    if req.new_password.trim().chars().count() < 8 {
        return (StatusCode::BAD_REQUEST, "new password must be at least 8 chars").into_response();
    }
    let cred = match st.creds.find_by_user_id(&user.user_id).await {
        Ok(Some(c)) => c,
        // Credential not stored: e.g. the built-in account injected via
        // SHEPHERD_ADMIN_PASSWORD; the store cannot change it.
        Ok(None) => {
            return (StatusCode::CONFLICT, "password managed by environment").into_response()
        }
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "auth error").into_response(),
    };
    if !st.hasher.verify(&req.old_password, &cred.password_hash) {
        return (StatusCode::UNAUTHORIZED, "wrong password").into_response();
    }
    let hash = st.hasher.hash(&req.new_password);
    match st.creds.update_password(&user.user_id, &hash).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::CONFLICT, "password managed by environment").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Deserialize, ToSchema)]
struct CreateUserRequest {
    name: String,
    email: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct UserResponse {
    id: String,
    name: String,
    email: String,
    enable: bool,
    /// Only populated by the list endpoint; always empty elsewhere.
    #[serde(default)]
    user_groups: Vec<String>,
}
impl From<crate::domain::User> for UserResponse {
    fn from(u: crate::domain::User) -> Self {
        Self {
            id: u.id,
            name: u.name,
            email: u.email.as_str().to_string(),
            enable: u.enable,
            user_groups: Vec::new(),
        }
    }
}

fn user_err_response(e: UserCmdError) -> Response {
    match e {
        UserCmdError::Validation(_) => (StatusCode::BAD_REQUEST, "invalid user").into_response(),
        UserCmdError::EmailExists => (StatusCode::CONFLICT, "email already exists").into_response(),
        UserCmdError::NotFound => (StatusCode::NOT_FOUND, "user not found").into_response(),
        UserCmdError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct UserPage {
    total: u64,
    current: u32,
    page_size: u32,
    total_pages: u64,
    items: Vec<UserResponse>,
}

#[utoipa::path(get, path = "/system/user", tag = "user", params(OrgListQuery), responses((status = 200, body = UserPage)), security(("bearer" = [])))]
async fn list_users(
    user: AuthUser,
    State(st): State<AppState>,
    Query(q): Query<OrgListQuery>,
) -> Response {
    if !user.can("SYSTEM_USER", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let page = match PageRequest::new(q.current, q.page_size) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid page params").into_response(),
    };
    match st.admin.list(page).await {
        Ok(p) => {
            let (total, current, page_size, total_pages) =
                (p.total, p.current, p.page_size, p.total_pages());
            let ids: Vec<String> = p.items.iter().map(|u| u.id.clone()).collect();
            let role_rows = st.user_roles.roles_for(&ids).await.unwrap_or_default();
            let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for (uid, name) in role_rows {
                groups.entry(uid).or_default().push(name);
            }
            let items = p
                .items
                .into_iter()
                .map(|u| {
                    let g = groups.get(&u.id).cloned().unwrap_or_default();
                    let mut resp = UserResponse::from(u);
                    resp.user_groups = g;
                    resp
                })
                .collect();
            let body = UserPage { total, current, page_size, total_pages, items };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => user_err_response(e),
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ResetPwResponse {
    password: String,
}

#[utoipa::path(post, path = "/system/user/{id}/reset-password", tag = "user", params(("id" = String, Path)), responses((status = 200, body = ResetPwResponse), (status = 404)), security(("bearer" = [])))]
async fn reset_password(
    user: AuthUser,
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("SYSTEM_USER", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let target = match st.admin.get(&id).await {
        Ok(u) => u,
        Err(e) => return user_err_response(e),
    };
    let new_pw = "Shepherd@123";
    let hash = st.hasher.hash(new_pw);
    match st.creds.reset_password(&id, target.email.as_str(), &hash).await {
        Ok(()) => {
            (StatusCode::OK, Json(ResetPwResponse { password: new_pw.to_string() })).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(get, path = "/system/user/{id}", tag = "user", params(("id" = String, Path)), responses((status = 200, body = UserResponse), (status = 404)), security(("bearer" = [])))]
async fn get_user(user: AuthUser, State(st): State<AppState>, Path(id): Path<String>) -> Response {
    if !user.can("SYSTEM_USER", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.get(&id).await {
        Ok(u) => (StatusCode::OK, Json(UserResponse::from(u))).into_response(),
        Err(e) => user_err_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct UpdateUserBody {
    name: String,
    email: String,
    #[serde(default = "tru")]
    enable: bool,
}
fn tru() -> bool {
    true
}

#[utoipa::path(put, path = "/system/user/{id}", tag = "user", params(("id" = String, Path)), request_body = UpdateUserBody, responses((status = 200, body = UserResponse), (status = 404)), security(("bearer" = [])))]
async fn update_user(
    user: AuthUser,
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateUserBody>,
) -> Response {
    if !user.can("SYSTEM_USER", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.update(&id, &req.name, &req.email, req.enable).await {
        Ok(u) => (StatusCode::OK, Json(UserResponse::from(u))).into_response(),
        Err(e) => user_err_response(e),
    }
}

#[utoipa::path(delete, path = "/system/user/{id}", tag = "user", params(("id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn delete_user(
    user: AuthUser,
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("SYSTEM_USER", "DELETE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.delete(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => user_err_response(e),
    }
}

#[utoipa::path(post, path = "/system/user", tag = "user", request_body = CreateUserRequest, responses((status = 201, body = UserResponse), (status = 409)), security(("bearer" = [])))]
async fn create_user(
    user: AuthUser,
    State(st): State<AppState>,
    Json(req): Json<CreateUserRequest>,
) -> Response {
    if !user.can("SYSTEM_USER", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied: SYSTEM_USER:ADD").into_response();
    }
    match st.create.execute(&req.name, &req.email).await {
        Ok(u) => (StatusCode::CREATED, Json(UserResponse::from(u))).into_response(),
        Err(CreateUserError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid user payload").into_response()
        }
        Err(CreateUserError::EmailAlreadyExists) => {
            (StatusCode::CONFLICT, "email already exists").into_response()
        }
        Err(CreateUserError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Deserialize, IntoParams)]
struct NamesQuery {
    #[serde(default)]
    ids: String,
}

// Any valid session suffices (display names are needed by every logged-in page), but it
// is no longer open to anonymous access.
#[utoipa::path(get, path = "/system/user/names", tag = "user", params(NamesQuery), responses((status = 200), (status = 401)), security(("bearer" = [])))]
async fn resolve_names(
    _user: AuthUser,
    State(st): State<AppState>,
    Query(q): Query<NamesQuery>,
) -> Response {
    let ids: Vec<String> =
        q.ids.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
    let names: BTreeMap<String, String> = st.resolve.execute(&ids).await;
    (StatusCode::OK, Json(names)).into_response()
}

pub fn oidc_router(oidc: OidcLoginUseCase) -> Router {
    Router::new()
        .route("/auth/oidc/{provider}/authorize", get(oidc_authorize))
        .route("/auth/oidc/{provider}/callback", get(oidc_callback))
        .route("/auth/oidc/providers", get(oidc_providers_public))
        .with_state(oidc)
}

/// Public, unauthenticated: lists the keys of providers currently registered
/// in the live runtime registry (so a login page can render SSO buttons).
#[utoipa::path(get, path = "/auth/oidc/providers", tag = "auth", responses((status = 200)))]
async fn oidc_providers_public(State(uc): State<OidcLoginUseCase>) -> Response {
    (StatusCode::OK, Json(uc.provider_keys())).into_response()
}

const STATE_COOKIE: &str = "oidc_state";

fn cookie_lookup(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(COOKIE)?.to_str().ok()?;
    raw.split(';')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| k.trim() == name)
        .map(|(_, v)| v.trim().to_string())
}

fn set_cookie(resp: &mut Response, value: &str) {
    if let Ok(v) = HeaderValue::from_str(value) {
        resp.headers_mut().insert(SET_COOKIE, v);
    }
}

#[utoipa::path(get, path = "/auth/oidc/{provider}/authorize", tag = "auth", params(("provider" = String, Path)), responses((status = 302), (status = 404)))]
async fn oidc_authorize(
    State(uc): State<OidcLoginUseCase>,
    Path(provider): Path<String>,
) -> Response {
    // The server generates an unguessable state, carried across the redirect via an
    // HttpOnly cookie and checked in the callback (CSRF protection).
    let state = uuid::Uuid::new_v4().simple().to_string();
    match uc.authorize_url(&provider, &state) {
        Ok(url) => {
            let mut resp = Redirect::to(&url).into_response();
            set_cookie(
                &mut resp,
                &format!(
                    "{STATE_COOKIE}={state}; HttpOnly; SameSite=Lax; Path=/auth/oidc; Max-Age=600"
                ),
            );
            resp
        }
        Err(_) => (StatusCode::NOT_FOUND, "unknown provider").into_response(),
    }
}

#[derive(Deserialize, IntoParams)]
struct CallbackQuery {
    code: String,
    #[serde(default)]
    state: String,
}

#[utoipa::path(get, path = "/auth/oidc/{provider}/callback", tag = "auth", params(("provider" = String, Path), CallbackQuery), responses((status = 200), (status = 400), (status = 401)))]
async fn oidc_callback(
    State(uc): State<OidcLoginUseCase>,
    Path(provider): Path<String>,
    Query(q): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Response {
    // CSRF: the callback state must be non-empty and match the cookie set by authorize.
    if q.state.is_empty()
        || cookie_lookup(&headers, STATE_COOKIE).as_deref() != Some(q.state.as_str())
    {
        return (StatusCode::BAD_REQUEST, "invalid or missing state").into_response();
    }
    let mut resp = match uc.complete(&provider, &q.code).await {
        Ok(token) => (StatusCode::OK, Json(LoginResponse { token, user_id: None })).into_response(),
        Err(OidcError::UnknownProvider(_)) => {
            (StatusCode::NOT_FOUND, "unknown provider").into_response()
        }
        Err(OidcError::Exchange(_)) => {
            (StatusCode::UNAUTHORIZED, "identity exchange failed").into_response()
        }
        Err(OidcError::Backend(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "auth error").into_response()
        }
    };
    // Single-use state: cleared after use.
    set_cookie(&mut resp, &format!("{STATE_COOKIE}=; Max-Age=0; Path=/auth/oidc"));
    resp
}

// ---------------------------------------------------------------------------
// OIDC provider admin API (runtime-editable system settings)
// ---------------------------------------------------------------------------
// Gated on `oidc` because mutations rebuild the live registry via the
// `build_provider` adapter factory. The public read route above is NOT gated
// so a login page can discover providers without the `oidc` feature... but in
// practice the server enables both together.

/// Live OIDC provider admin state: the dynamic registry plus the repository
/// that backs it. Mutations rebuild the registry via `reload`.
#[derive(Clone)]
#[cfg(all(feature = "http", feature = "oidc"))]
struct OidcAdminState {
    oidc: OidcLoginUseCase,
    repo: Arc<dyn OidcProviderRepository>,
    sessions: Arc<dyn SessionStore>,
}

#[cfg(all(feature = "http", feature = "oidc"))]
impl FromRef<OidcAdminState> for Arc<dyn SessionStore> {
    fn from_ref(s: &OidcAdminState) -> Self {
        s.sessions.clone()
    }
}

/// Provider record as returned by the admin API. `app_secret` is never
/// serialized; `secret_set` tells the client whether a secret is configured.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[cfg(all(feature = "http", feature = "oidc"))]
struct OidcProviderView {
    provider_key: String,
    app_id: String,
    redirect: String,
    default_permissions: Vec<String>,
    enabled: bool,
    base_url: Option<String>,
    secret_set: bool,
}

#[cfg(all(feature = "http", feature = "oidc"))]
impl OidcProviderView {
    fn from_provider(p: &OidcProvider) -> Self {
        Self {
            provider_key: p.provider_key.clone(),
            app_id: p.app_id.clone(),
            redirect: p.redirect.clone(),
            default_permissions: p.default_permissions.clone(),
            enabled: p.enabled,
            base_url: p.base_url.clone(),
            secret_set: !p.app_secret.is_empty(),
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[cfg(all(feature = "http", feature = "oidc"))]
struct OidcProviderCreate {
    provider_key: String,
    app_id: String,
    app_secret: String,
    #[serde(default)]
    redirect: String,
    #[serde(default)]
    default_permissions: Vec<String>,
    #[serde(default = "oidc_default_true")]
    enabled: bool,
    #[serde(default)]
    base_url: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[cfg(all(feature = "http", feature = "oidc"))]
struct OidcProviderUpdate {
    app_id: String,
    #[serde(default)]
    app_secret: Option<String>,
    #[serde(default)]
    redirect: String,
    #[serde(default)]
    default_permissions: Vec<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    base_url: Option<String>,
}

#[cfg(all(feature = "http", feature = "oidc"))]
fn oidc_default_true() -> bool {
    true
}

#[cfg(all(feature = "http", feature = "oidc"))]
fn oidc_repo_err(e: OidcRepoError) -> Response {
    match e {
        OidcRepoError::NotFound => (StatusCode::NOT_FOUND, "provider not found").into_response(),
        OidcRepoError::Backend(m) => {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("storage error: {m}")).into_response()
        }
    }
}

/// Rebuilds the live registry from the repository after a mutation.
#[cfg(all(feature = "http", feature = "oidc"))]
async fn oidc_reload_after_mutate(st: &OidcAdminState) -> Result<(), Response> {
    st.oidc.reload(st.repo.as_ref(), build_provider).await.map_err(oidc_repo_err)
}

#[utoipa::path(get, path = "/system/oidc/providers", tag = "oidc", responses((status = 200, body = Vec<OidcProviderView>), (status = 401), (status = 403)), security(("bearer" = [])))]
#[cfg(all(feature = "http", feature = "oidc"))]
async fn oidc_list_providers(user: AuthUser, State(st): State<OidcAdminState>) -> Response {
    if !user.can("SYSTEM_USER", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied: SYSTEM_USER:READ").into_response();
    }
    match st.repo.list().await {
        Ok(items) => (
            StatusCode::OK,
            Json(items.iter().map(OidcProviderView::from_provider).collect::<Vec<_>>()),
        )
            .into_response(),
        Err(e) => oidc_repo_err(e),
    }
}

#[utoipa::path(get, path = "/system/oidc/providers/{key}", tag = "oidc", params(("key" = String, Path)), responses((status = 200, body = OidcProviderView), (status = 401), (status = 403), (status = 404)), security(("bearer" = [])))]
#[cfg(all(feature = "http", feature = "oidc"))]
async fn oidc_get_provider(
    user: AuthUser,
    State(st): State<OidcAdminState>,
    Path(key): Path<String>,
) -> Response {
    if !user.can("SYSTEM_USER", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied: SYSTEM_USER:READ").into_response();
    }
    match st.repo.get(&key).await {
        Ok(Some(p)) => (StatusCode::OK, Json(OidcProviderView::from_provider(&p))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "provider not found").into_response(),
        Err(e) => oidc_repo_err(e),
    }
}

#[utoipa::path(post, path = "/system/oidc/providers", tag = "oidc", responses((status = 201, body = OidcProviderView), (status = 400), (status = 401), (status = 403)), security(("bearer" = [])))]
#[cfg(all(feature = "http", feature = "oidc"))]
async fn oidc_create_provider(
    user: AuthUser,
    State(st): State<OidcAdminState>,
    Json(req): Json<OidcProviderCreate>,
) -> Response {
    if !user.can("SYSTEM_USER", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied: SYSTEM_USER:UPDATE").into_response();
    }
    let provider = OidcProvider {
        provider_key: req.provider_key.trim().to_string(),
        app_id: req.app_id.trim().to_string(),
        app_secret: req.app_secret,
        redirect: req.redirect,
        default_permissions: req.default_permissions,
        enabled: req.enabled,
        base_url: req.base_url,
    };
    if provider.provider_key.is_empty() {
        return (StatusCode::BAD_REQUEST, "provider_key is required").into_response();
    }
    if build_provider(&provider).is_none() {
        return (StatusCode::BAD_REQUEST, "unsupported provider key").into_response();
    }
    if let Err(e) = st.repo.upsert(&provider).await {
        return oidc_repo_err(e);
    }
    if let Err(e) = oidc_reload_after_mutate(&st).await {
        return e;
    }
    (StatusCode::CREATED, Json(OidcProviderView::from_provider(&provider))).into_response()
}

#[utoipa::path(put, path = "/system/oidc/providers/{key}", tag = "oidc", params(("key" = String, Path)), responses((status = 200, body = OidcProviderView), (status = 400), (status = 401), (status = 403), (status = 404)), security(("bearer" = [])))]
#[cfg(all(feature = "http", feature = "oidc"))]
async fn oidc_update_provider(
    user: AuthUser,
    State(st): State<OidcAdminState>,
    Path(key): Path<String>,
    Json(req): Json<OidcProviderUpdate>,
) -> Response {
    if !user.can("SYSTEM_USER", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied: SYSTEM_USER:UPDATE").into_response();
    }
    let existing = match st.repo.get(&key).await {
        Ok(Some(p)) => p,
        Ok(None) => return (StatusCode::NOT_FOUND, "provider not found").into_response(),
        Err(e) => return oidc_repo_err(e),
    };
    let provider = OidcProvider {
        provider_key: key,
        app_id: req.app_id.trim().to_string(),
        app_secret: req.app_secret.unwrap_or(existing.app_secret),
        redirect: req.redirect,
        default_permissions: req.default_permissions,
        enabled: req.enabled.unwrap_or(existing.enabled),
        base_url: req.base_url,
    };
    if build_provider(&provider).is_none() {
        return (StatusCode::BAD_REQUEST, "unsupported provider key").into_response();
    }
    if let Err(e) = st.repo.upsert(&provider).await {
        return oidc_repo_err(e);
    }
    if let Err(e) = oidc_reload_after_mutate(&st).await {
        return e;
    }
    (StatusCode::OK, Json(OidcProviderView::from_provider(&provider))).into_response()
}

#[utoipa::path(delete, path = "/system/oidc/providers/{key}", tag = "oidc", params(("key" = String, Path)), responses((status = 204), (status = 401), (status = 403), (status = 404)), security(("bearer" = [])))]
#[cfg(all(feature = "http", feature = "oidc"))]
async fn oidc_delete_provider(
    user: AuthUser,
    State(st): State<OidcAdminState>,
    Path(key): Path<String>,
) -> Response {
    if !user.can("SYSTEM_USER", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied: SYSTEM_USER:UPDATE").into_response();
    }
    match st.repo.delete(&key).await {
        Ok(()) => {
            if let Err(e) = oidc_reload_after_mutate(&st).await {
                return e;
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => oidc_repo_err(e),
    }
}

/// Admin router for managing OIDC providers as runtime system settings.
#[cfg(all(feature = "http", feature = "oidc"))]
pub fn oidc_admin_router(
    oidc: OidcLoginUseCase,
    repo: Arc<dyn OidcProviderRepository>,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/system/oidc/providers", get(oidc_list_providers).post(oidc_create_provider))
        .route(
            "/system/oidc/providers/{key}",
            get(oidc_get_provider).put(oidc_update_provider).delete(oidc_delete_provider),
        )
        .with_state(OidcAdminState { oidc, repo, sessions })
}

#[derive(Clone)]
struct OrgState {
    svc: OrganizationService,
    sessions: Arc<dyn SessionStore>,
}
impl FromRef<OrgState> for Arc<dyn SessionStore> {
    fn from_ref(s: &OrgState) -> Self {
        s.sessions.clone()
    }
}

pub fn org_router(svc: OrganizationService, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/organization", post(create_org).get(list_orgs))
        .route("/organization/{id}", get(get_org).put(update_org).delete(delete_org))
        .with_state(OrgState { svc, sessions })
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct OrgResponse {
    id: String,
    name: String,
    enable: bool,
}
impl From<crate::domain::Organization> for OrgResponse {
    fn from(o: crate::domain::Organization) -> Self {
        Self { id: o.id, name: o.name, enable: o.enable }
    }
}

fn org_err_response(e: OrgCmdError) -> Response {
    match e {
        OrgCmdError::Validation(_) => {
            (StatusCode::BAD_REQUEST, "invalid organization").into_response()
        }
        OrgCmdError::NameExists => (StatusCode::CONFLICT, "name already exists").into_response(),
        OrgCmdError::NotFound => (StatusCode::NOT_FOUND, "organization not found").into_response(),
        OrgCmdError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Deserialize, ToSchema)]
struct OrgBody {
    name: String,
    #[serde(default = "default_true")]
    enable: bool,
}
fn default_true() -> bool {
    true
}

#[utoipa::path(post, path = "/organization", tag = "organization", request_body = OrgBody, responses((status = 201, body = OrgResponse), (status = 409)), security(("bearer" = [])))]
async fn create_org(
    user: AuthUser,
    State(st): State<OrgState>,
    Json(req): Json<OrgBody>,
) -> Response {
    if !user.can("ORGANIZATION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.create(&req.name).await {
        Ok(o) => (StatusCode::CREATED, Json(OrgResponse::from(o))).into_response(),
        Err(e) => org_err_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct OrgListQuery {
    #[serde(default = "one")]
    current: u32,
    #[serde(default = "ten")]
    page_size: u32,
}
fn one() -> u32 {
    1
}
fn ten() -> u32 {
    10
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct OrgPage {
    total: u64,
    current: u32,
    page_size: u32,
    total_pages: u64,
    items: Vec<OrgResponse>,
}

#[utoipa::path(get, path = "/organization", tag = "organization", params(OrgListQuery), responses((status = 200, body = OrgPage)), security(("bearer" = [])))]
async fn list_orgs(
    user: AuthUser,
    State(st): State<OrgState>,
    Query(q): Query<OrgListQuery>,
) -> Response {
    if !user.can("ORGANIZATION", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let page = match PageRequest::new(q.current, q.page_size) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid page params").into_response(),
    };
    match st.svc.list(page).await {
        Ok(p) => (
            StatusCode::OK,
            Json(OrgPage {
                total: p.total,
                current: p.current,
                page_size: p.page_size,
                total_pages: p.total_pages(),
                items: p.items.into_iter().map(OrgResponse::from).collect(),
            }),
        )
            .into_response(),
        Err(e) => org_err_response(e),
    }
}

#[utoipa::path(get, path = "/organization/{id}", tag = "organization", params(("id" = String, Path)), responses((status = 200, body = OrgResponse), (status = 404)), security(("bearer" = [])))]
async fn get_org(user: AuthUser, State(st): State<OrgState>, Path(id): Path<String>) -> Response {
    if !user.can("ORGANIZATION", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.get(&id).await {
        Ok(o) => (StatusCode::OK, Json(OrgResponse::from(o))).into_response(),
        Err(e) => org_err_response(e),
    }
}

#[utoipa::path(put, path = "/organization/{id}", tag = "organization", params(("id" = String, Path)), request_body = OrgBody, responses((status = 200, body = OrgResponse), (status = 404)), security(("bearer" = [])))]
async fn update_org(
    user: AuthUser,
    State(st): State<OrgState>,
    Path(id): Path<String>,
    Json(req): Json<OrgBody>,
) -> Response {
    if !user.can("ORGANIZATION", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.update(&id, &req.name, req.enable).await {
        Ok(o) => (StatusCode::OK, Json(OrgResponse::from(o))).into_response(),
        Err(e) => org_err_response(e),
    }
}

#[utoipa::path(delete, path = "/organization/{id}", tag = "organization", params(("id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn delete_org(
    user: AuthUser,
    State(st): State<OrgState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("ORGANIZATION", "DELETE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.delete(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => org_err_response(e),
    }
}

#[derive(Clone)]
struct RoleState {
    roles: RoleService,
    grants: UserRoleService,
    sessions: Arc<dyn SessionStore>,
}
impl FromRef<RoleState> for Arc<dyn SessionStore> {
    fn from_ref(s: &RoleState) -> Self {
        s.sessions.clone()
    }
}

pub fn role_router(
    roles: RoleService,
    grants: UserRoleService,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/role", post(create_role).get(list_roles))
        .route("/role/{id}", get(get_role).put(update_role).delete(delete_role))
        .route("/user-role/grant", post(grant_role))
        .route("/user-role/revoke", post(revoke_role))
        .with_state(RoleState { roles, grants, sessions })
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RoleResponse {
    id: String,
    name: String,
    scope: String,
    permissions: Vec<String>,
}
impl From<crate::domain::Role> for RoleResponse {
    fn from(r: crate::domain::Role) -> Self {
        Self {
            id: r.id,
            name: r.name,
            scope: r.scope.as_str().to_string(),
            permissions: r.permissions,
        }
    }
}

fn role_err_response(e: RoleCmdError) -> Response {
    match e {
        RoleCmdError::Validation(_) => (StatusCode::BAD_REQUEST, "invalid role").into_response(),
        RoleCmdError::NotFound => (StatusCode::NOT_FOUND, "role not found").into_response(),
        RoleCmdError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Deserialize, ToSchema)]
struct RoleBody {
    name: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    permissions: Vec<String>,
}

#[utoipa::path(post, path = "/role", tag = "role", request_body = RoleBody, responses((status = 201, body = RoleResponse), (status = 409)), security(("bearer" = [])))]
async fn create_role(
    user: AuthUser,
    State(st): State<RoleState>,
    Json(req): Json<RoleBody>,
) -> Response {
    if !user.can("USER_ROLE", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let scope = match RoleScope::parse(req.scope.as_deref().unwrap_or("SYSTEM")) {
        Some(s) => s,
        None => return (StatusCode::BAD_REQUEST, "invalid scope").into_response(),
    };
    match st.roles.create(&req.name, scope, req.permissions).await {
        Ok(r) => (StatusCode::CREATED, Json(RoleResponse::from(r))).into_response(),
        Err(e) => role_err_response(e),
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RolePage {
    total: u64,
    current: u32,
    page_size: u32,
    total_pages: u64,
    items: Vec<RoleResponse>,
}

#[utoipa::path(get, path = "/role", tag = "role", params(OrgListQuery), responses((status = 200, body = RolePage)), security(("bearer" = [])))]
async fn list_roles(
    user: AuthUser,
    State(st): State<RoleState>,
    Query(q): Query<OrgListQuery>,
) -> Response {
    if !user.can("USER_ROLE", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let page = match PageRequest::new(q.current, q.page_size) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid page params").into_response(),
    };
    match st.roles.list(page).await {
        Ok(p) => {
            let body = RolePage {
                total: p.total,
                current: p.current,
                page_size: p.page_size,
                total_pages: p.total_pages(),
                items: p.items.into_iter().map(RoleResponse::from).collect(),
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => role_err_response(e),
    }
}

#[utoipa::path(get, path = "/role/{id}", tag = "role", params(("id" = String, Path)), responses((status = 200, body = RoleResponse), (status = 404)), security(("bearer" = [])))]
async fn get_role(user: AuthUser, State(st): State<RoleState>, Path(id): Path<String>) -> Response {
    if !user.can("USER_ROLE", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.roles.get(&id).await {
        Ok(r) => (StatusCode::OK, Json(RoleResponse::from(r))).into_response(),
        Err(e) => role_err_response(e),
    }
}

#[utoipa::path(put, path = "/role/{id}", tag = "role", params(("id" = String, Path)), request_body = RoleBody, responses((status = 200, body = RoleResponse), (status = 404)), security(("bearer" = [])))]
async fn update_role(
    user: AuthUser,
    State(st): State<RoleState>,
    Path(id): Path<String>,
    Json(req): Json<RoleBody>,
) -> Response {
    if !user.can("USER_ROLE", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.roles.update(&id, &req.name, req.permissions).await {
        Ok(r) => (StatusCode::OK, Json(RoleResponse::from(r))).into_response(),
        Err(e) => role_err_response(e),
    }
}

#[utoipa::path(delete, path = "/role/{id}", tag = "role", params(("id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn delete_role(
    user: AuthUser,
    State(st): State<RoleState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("USER_ROLE", "DELETE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.roles.delete(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => role_err_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct GrantBody {
    user_id: String,
    role_id: String,
}

#[utoipa::path(post, path = "/user-role/grant", tag = "role", request_body = GrantBody, responses((status = 200)), security(("bearer" = [])))]
async fn grant_role(
    user: AuthUser,
    State(st): State<RoleState>,
    Json(req): Json<GrantBody>,
) -> Response {
    if !user.can("USER_ROLE", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.grants.grant(&req.user_id, &req.role_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => role_err_response(e),
    }
}

#[utoipa::path(post, path = "/user-role/revoke", tag = "role", request_body = GrantBody, responses((status = 200)), security(("bearer" = [])))]
async fn revoke_role(
    user: AuthUser,
    State(st): State<RoleState>,
    Json(req): Json<GrantBody>,
) -> Response {
    if !user.can("USER_ROLE", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.grants.revoke(&req.user_id, &req.role_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => role_err_response(e),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        login_handler, logout_handler, refresh_handler, me_handler, change_password,
        resolve_names, oidc_authorize, oidc_callback,
        create_user, list_users, get_user, update_user, delete_user,
        create_org, list_orgs, get_org, update_org, delete_org,
        create_role, list_roles, get_role, update_role, delete_role, grant_role, revoke_role
    ),
    components(schemas(LoginRequest, LoginResponse, MeResponse, ChangePasswordBody, CreateUserRequest, UserResponse, UserPage, UpdateUserBody, OrgResponse, OrgBody, OrgPage, RoleResponse, RoleBody, RolePage, GrantBody)),
    tags((name = "auth", description = "鉴权"), (name = "user", description = "用户管理"), (name = "organization", description = "组织"), (name = "role", description = "角色 / 授权"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = ApiDoc::openapi();
    doc.merge(super::apikey_http::openapi());
    doc.merge(super::llm_model_http::openapi());
    #[cfg(all(feature = "http", feature = "oidc"))]
    doc.merge(oidc_openapi());
    doc
}

/// OpenAPI fragment for the OIDC provider admin endpoints. Gated on `oidc`
/// because the handlers and DTOs only exist when that feature is enabled.
#[cfg(all(feature = "http", feature = "oidc"))]
#[derive(OpenApi)]
#[openapi(
    paths(oidc_list_providers, oidc_get_provider, oidc_create_provider, oidc_update_provider, oidc_delete_provider),
    components(schemas(OidcProviderView, OidcProviderCreate, OidcProviderUpdate)),
    tags((name = "oidc", description = "OIDC 身份提供方"))
)]
struct OidcApiDoc;

#[cfg(all(feature = "http", feature = "oidc"))]
fn oidc_openapi() -> utoipa::openapi::OpenApi {
    OidcApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{
        FakeIdentityProvider, InMemoryCredentialRepository, InMemoryExternalUserRepository,
        InMemoryOrgRepository, InMemoryRoleRepository, InMemorySessionStore,
        InMemoryUserRepository, InMemoryUserRoleRepository, PlainPasswordHasher, SpyDirectory,
    };
    use crate::domain::ExternalIdentity;
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;

    fn app_store() -> (Router, Arc<InMemorySessionStore>) {
        let user_repo = Arc::new(InMemoryUserRepository::new());
        let create = CreateUserUseCase::new(user_repo.clone());
        let admin = UserService::new(user_repo);
        let resolve = ResolveUserNamesUseCase::new(Arc::new(SpyDirectory::new()));
        let creds = Arc::new(
            InMemoryCredentialRepository::new()
                .with_user("admin", "u-admin", "secret", ["SYSTEM_USER:READ+ADD+UPDATE+DELETE"])
                .with_user("viewer", "u-view", "pw", ["SYSTEM_USER:READ"]),
        );
        let hasher = Arc::new(PlainPasswordHasher);
        let sessions = Arc::new(InMemorySessionStore::new());
        let user_roles =
            Arc::new(InMemoryUserRoleRepository::new(Arc::new(InMemoryRoleRepository::new())));
        let login =
            LoginUseCase::new(creds.clone(), hasher.clone(), sessions.clone(), user_roles.clone());
        (
            router(
                create,
                resolve,
                login,
                admin,
                creds,
                hasher,
                user_roles,
                sessions.clone(),
                3600,
            ),
            sessions,
        )
    }

    fn app() -> Router {
        app_store().0
    }

    fn post(uri: &str, body: &str, bearer: Option<&str>) -> Request<Body> {
        let mut b =
            Request::builder().method("POST").uri(uri).header("content-type", "application/json");
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    async fn token(app: &Router, user: &str, pass: &str) -> Option<String> {
        let resp = app
            .clone()
            .oneshot(post(
                "/auth/login",
                &format!(r#"{{"username":"{user}","password":"{pass}"}}"#),
                None,
            ))
            .await
            .expect("resp");
        if resp.status() != StatusCode::OK {
            return None;
        }
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        v["token"].as_str().map(str::to_string)
    }

    fn create_body() -> &'static str {
        r#"{"name":"Alice","email":"a@x.com"}"#
    }

    #[tokio::test]
    async fn login_ok_returns_token_wrong_401() {
        let app = app();
        assert!(token(&app, "admin", "secret").await.is_some());
        assert!(token(&app, "admin", "WRONG").await.is_none());
    }

    #[tokio::test]
    async fn login_lockout_maps_to_429() {
        let app = app();
        let body = r#"{"username":"admin","password":"WRONG"}"#;
        for _ in 0..crate::application::login::MAX_LOGIN_FAILURES {
            let r = app.clone().oneshot(post("/auth/login", body, None)).await.expect("r");
            assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        }
        // Once locked out, even the correct password gets 429.
        let locked = app
            .oneshot(post("/auth/login", r#"{"username":"admin","password":"secret"}"#, None))
            .await
            .expect("r");
        assert_eq!(locked.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn resolve_names_requires_session_but_no_permission() {
        let app = app();
        let anon = app
            .clone()
            .oneshot(req("GET", "/system/user/names?ids=u-admin", "", None))
            .await
            .expect("r");
        assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);
        // viewer only has READ permission, but any valid session can resolve display names.
        let t = token(&app, "viewer", "pw").await.expect("token");
        let ok = app
            .oneshot(req("GET", "/system/user/names?ids=u-admin", "", Some(&t)))
            .await
            .expect("r");
        assert_eq!(ok.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn refresh_rotates_token_and_revokes_old() {
        let (app, sessions) = app_store();
        let old = token(&app, "admin", "secret").await.expect("login");

        let resp = app.clone().oneshot(post("/auth/refresh", "", Some(&old))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let new = serde_json::from_slice::<serde_json::Value>(&bytes).expect("json")["token"]
            .as_str()
            .expect("token")
            .to_string();
        assert_ne!(new, old);
        // New token valid; old token revoked.
        assert!(sessions.get(&new).await.expect("ok").is_some());
        assert!(sessions.get(&old).await.expect("ok").is_none());

        // Refresh without a token → 401.
        let no_tok = app.oneshot(post("/auth/refresh", "", None)).await.expect("resp");
        assert_eq!(no_tok.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn me_returns_session_user_and_permissions() {
        let app = app();
        let t = token(&app, "viewer", "pw").await.expect("token");
        let resp = app.clone().oneshot(req("GET", "/auth/me", "", Some(&t))).await.expect("r");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["userId"], "u-view");
        assert_eq!(v["permissions"], serde_json::json!(["SYSTEM_USER:READ"]));

        let anon = app.oneshot(req("GET", "/auth/me", "", None)).await.expect("r");
        assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn change_password_happy_path_keeps_session_alive() {
        let app = app();
        let t = token(&app, "admin", "secret").await.expect("token");
        let body = r#"{"oldPassword":"secret","newPassword":"n3w-secret"}"#;
        let resp = app.clone().oneshot(post("/auth/password", body, Some(&t))).await.expect("r");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        // Old password stops working, new one logs in; existing sessions are not revoked.
        assert!(token(&app, "admin", "secret").await.is_none());
        assert!(token(&app, "admin", "n3w-secret").await.is_some());
        let still = app.oneshot(req("GET", "/auth/me", "", Some(&t))).await.expect("r");
        assert_eq!(still.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn change_password_wrong_old_is_401_weak_new_is_400() {
        let app = app();
        let t = token(&app, "admin", "secret").await.expect("token");
        let wrong = r#"{"oldPassword":"nope","newPassword":"n3w-secret"}"#;
        let r = app.clone().oneshot(post("/auth/password", wrong, Some(&t))).await.expect("r");
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        // Fewer than 8 chars after trim → 400 (rejected regardless of the old password).
        let weak = r#"{"oldPassword":"secret","newPassword":"  short1 "}"#;
        let r = app.clone().oneshot(post("/auth/password", weak, Some(&t))).await.expect("r");
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        // Unchanged: the old password still logs in.
        assert!(token(&app, "admin", "secret").await.is_some());
    }

    #[tokio::test]
    async fn change_password_env_managed_user_is_409() {
        let (app, sessions) = app_store();
        // Valid session but credential not stored → treated as an env-injected built-in account.
        let t = sessions
            .create("u-env", kernel::permission::PermissionSet::default(), 3600)
            .await
            .expect("s");
        let body = r#"{"oldPassword":"whatever","newPassword":"n3w-secret"}"#;
        let r = app.oneshot(post("/auth/password", body, Some(&t))).await.expect("r");
        assert_eq!(r.status(), StatusCode::CONFLICT);
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.expect("body");
        assert_eq!(bytes.as_ref(), b"password managed by environment");
    }

    #[tokio::test]
    async fn create_user_without_token_is_401() {
        let resp = app().oneshot(post("/system/user", create_body(), None)).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_user_with_admin_token_is_201() {
        let app = app();
        let t = token(&app, "admin", "secret").await.expect("token");
        let resp = app.oneshot(post("/system/user", create_body(), Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn create_user_with_insufficient_permission_is_403() {
        let app = app();
        let t = token(&app, "viewer", "pw").await.expect("token");
        let resp = app.oneshot(post("/system/user", create_body(), Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn create_user_with_bogus_token_is_401() {
        let resp = app()
            .oneshot(post("/system/user", create_body(), Some("not-a-real-token")))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn logout_revokes_token() {
        let app = app();
        let t = token(&app, "admin", "secret").await.expect("token");
        assert_eq!(
            app.clone()
                .oneshot(post("/system/user", create_body(), Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::CREATED
        );
        let lo = app.clone().oneshot(post("/auth/logout", "", Some(&t))).await.expect("r");
        assert_eq!(lo.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            app.oneshot(post("/system/user", create_body(), Some(&t))).await.expect("r").status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn expired_session_is_401() {
        let (app, store) = app_store();
        store.insert_expired("expired-tok", "u-admin");
        let resp = app
            .oneshot(post("/system/user", create_body(), Some("expired-tok")))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    fn oidc_app() -> Router {
        let users = Arc::new(InMemoryExternalUserRepository::new(["PROJECT:READ"]));
        let sessions = Arc::new(InMemorySessionStore::new());
        let feishu = FakeIdentityProvider::new(
            "feishu",
            ExternalIdentity {
                provider: "feishu".into(),
                open_id: "ou_x".into(),
                display_name: "X".into(),
            },
        );
        oidc_router(OidcLoginUseCase::new(users, sessions).register(Arc::new(feishu)))
    }

    fn get_req(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).expect("req")
    }

    fn state_cookie(resp: &Response) -> String {
        let c = resp.headers().get("set-cookie").expect("set-cookie").to_str().expect("s");
        c.strip_prefix("oidc_state=").expect("prefix").split(';').next().expect("v").to_string()
    }

    fn cb_req(code: &str, q_state: &str, cookie: Option<&str>) -> Request<Body> {
        let mut b = Request::builder()
            .uri(format!("/auth/oidc/feishu/callback?code={code}&state={q_state}"));
        if let Some(c) = cookie {
            b = b.header("cookie", format!("oidc_state={c}"));
        }
        b.body(Body::empty()).expect("req")
    }

    #[tokio::test]
    async fn oidc_authorize_mints_state_cookie_and_redirects() {
        let resp = oidc_app().oneshot(get_req("/auth/oidc/feishu/authorize")).await.expect("r");
        assert!(resp.status().is_redirection());
        let loc = resp.headers().get("location").expect("loc").to_str().expect("s").to_string();
        let cookie = resp.headers().get("set-cookie").expect("cookie").to_str().expect("s");
        assert!(cookie.starts_with("oidc_state=") && cookie.contains("HttpOnly"), "{cookie}");
        let state = state_cookie(&resp);
        assert!(loc.contains("fake.example") && loc.contains(&format!("state={state}")), "{loc}");
    }

    #[tokio::test]
    async fn oidc_authorize_unknown_provider_404() {
        let resp = oidc_app().oneshot(get_req("/auth/oidc/github/authorize")).await.expect("r");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn oidc_callback_good_code_with_matching_state_returns_token() {
        let app = oidc_app();
        let auth = app.clone().oneshot(get_req("/auth/oidc/feishu/authorize")).await.expect("r");
        let state = state_cookie(&auth);
        let resp = app.oneshot(cb_req("ok", &state, Some(&state))).await.expect("r");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(v["token"].as_str().expect("tok").starts_with("sess-"));
    }

    #[tokio::test]
    async fn oidc_callback_rejects_state_mismatch_or_missing_cookie() {
        // Cookie and query state mismatch → 400 (CSRF).
        let r1 = oidc_app().oneshot(cb_req("ok", "attacker", Some("real"))).await.expect("r");
        assert_eq!(r1.status(), StatusCode::BAD_REQUEST);
        // No cookie → 400.
        let r2 = oidc_app().oneshot(cb_req("ok", "s", None)).await.expect("r");
        assert_eq!(r2.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn oidc_callback_bad_code_401_with_valid_state() {
        let app = oidc_app();
        let auth = app.clone().oneshot(get_req("/auth/oidc/feishu/authorize")).await.expect("r");
        let state = state_cookie(&auth);
        let resp = app.oneshot(cb_req("bad", &state, Some(&state))).await.expect("r");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // --- OIDC provider admin API (feature-gated on `oidc`) -------------------
    #[cfg(all(feature = "http", feature = "oidc"))]
    use async_trait::async_trait;

    #[cfg(all(feature = "http", feature = "oidc"))]
    struct MemOidcRepo {
        inner: std::sync::Mutex<Vec<OidcProvider>>,
    }

    #[cfg(all(feature = "http", feature = "oidc"))]
    impl MemOidcRepo {
        fn new() -> Self {
            Self { inner: std::sync::Mutex::new(Vec::new()) }
        }
    }

    #[cfg(all(feature = "http", feature = "oidc"))]
    #[async_trait]
    impl OidcProviderRepository for MemOidcRepo {
        async fn list(&self) -> Result<Vec<OidcProvider>, OidcRepoError> {
            Ok(self.inner.lock().expect("poisoned").clone())
        }
        async fn list_enabled(&self) -> Result<Vec<OidcProvider>, OidcRepoError> {
            Ok(self.inner.lock().expect("poisoned").iter().filter(|p| p.enabled).cloned().collect())
        }
        async fn get(&self, key: &str) -> Result<Option<OidcProvider>, OidcRepoError> {
            Ok(self.inner.lock().expect("poisoned").iter().find(|p| p.provider_key == key).cloned())
        }
        async fn upsert(&self, p: &OidcProvider) -> Result<(), OidcRepoError> {
            let mut v = self.inner.lock().expect("poisoned");
            if let Some(existing) = v.iter_mut().find(|x| x.provider_key == p.provider_key) {
                *existing = p.clone();
            } else {
                v.push(p.clone());
            }
            Ok(())
        }
        async fn delete(&self, key: &str) -> Result<(), OidcRepoError> {
            self.inner.lock().expect("poisoned").retain(|x| x.provider_key != key);
            Ok(())
        }
    }

    #[cfg(all(feature = "http", feature = "oidc"))]
    async fn oidc_admin_app() -> (Router, String, String) {
        let users = Arc::new(InMemoryExternalUserRepository::new(["PROJECT:READ"]));
        let sessions = Arc::new(InMemorySessionStore::new());
        // Registry starts empty; mutations rebuild it via reload (exercised below).
        let oidc = OidcLoginUseCase::new(users, sessions.clone());
        let repo = Arc::new(MemOidcRepo::new());
        let router = oidc_admin_router(oidc, repo, sessions.clone());
        let admin = sessions
            .create(
                "u-admin",
                PermissionSet::from_raw(["SYSTEM_USER:READ+UPDATE"]).expect("admin perms"),
                3600,
            )
            .await
            .expect("admin session");
        let viewer = sessions
            .create(
                "u-view",
                PermissionSet::from_raw(["SYSTEM_USER:READ"]).expect("viewer perms"),
                3600,
            )
            .await
            .expect("viewer session");
        (router, admin, viewer)
    }

    #[tokio::test]
    #[cfg(all(feature = "http", feature = "oidc"))]
    async fn oidc_admin_crud_and_permission_flow() {
        let (app, admin, viewer) = oidc_admin_app().await;

        // Anonymous → 401 on the list endpoint.
        let anon =
            app.clone().oneshot(req("GET", "/system/oidc/providers", "", None)).await.expect("r");
        assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);

        // Viewer (READ only) can list but cannot create (UPDATE required).
        let listed = app
            .clone()
            .oneshot(req("GET", "/system/oidc/providers", "", Some(&viewer)))
            .await
            .expect("r");
        assert_eq!(listed.status(), StatusCode::OK);
        let created = app
            .clone()
            .oneshot(req(
                "POST",
                "/system/oidc/providers",
                r#"{"providerKey":"feishu","appId":"cli_x","appSecret":"s3cr3t"}"#,
                Some(&viewer),
            ))
            .await
            .expect("r");
        assert_eq!(created.status(), StatusCode::FORBIDDEN);

        // Admin can create; secret is never echoed back (secret_set=true instead).
        let created = app
            .clone()
            .oneshot(req(
                "POST",
                "/system/oidc/providers",
                r#"{"providerKey":"feishu","appId":"cli_x","appSecret":"s3cr3t"}"#,
                Some(&admin),
            ))
            .await
            .expect("r");
        assert_eq!(created.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(created.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["providerKey"], "feishu");
        assert_eq!(v["appId"], "cli_x");
        assert!(v.get("appSecret").is_none(), "secret must not be serialized");
        assert_eq!(v["secretSet"], true);

        // The new provider is now listed; the live registry was rebuilt via reload.
        let listed = app
            .clone()
            .oneshot(req("GET", "/system/oidc/providers", "", Some(&admin)))
            .await
            .expect("r");
        assert_eq!(listed.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(listed.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v.as_array().expect("arr").len(), 1);

        // Get by key, then update (rotating the secret), then delete.
        let got = app
            .clone()
            .oneshot(req("GET", "/system/oidc/providers/feishu", "", Some(&admin)))
            .await
            .expect("r");
        assert_eq!(got.status(), StatusCode::OK);

        let updated = app
            .clone()
            .oneshot(req(
                "PUT",
                "/system/oidc/providers/feishu",
                r#"{"appId":"cli_y","appSecret":"new-s3cr3t","enabled":false}"#,
                Some(&admin),
            ))
            .await
            .expect("r");
        assert_eq!(updated.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(updated.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["appId"], "cli_y");
        assert_eq!(v["enabled"], false);

        // Unsupported provider key → 400 on create.
        let bad = app
            .clone()
            .oneshot(req(
                "POST",
                "/system/oidc/providers",
                r#"{"providerKey":"github","appId":"a","appSecret":"s"}"#,
                Some(&admin),
            ))
            .await
            .expect("r");
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

        // Delete removes it and rebuilds the registry again.
        let deleted = app
            .clone()
            .oneshot(req("DELETE", "/system/oidc/providers/feishu", "", Some(&admin)))
            .await
            .expect("r");
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
        let listed =
            app.oneshot(req("GET", "/system/oidc/providers", "", Some(&admin))).await.expect("r");
        assert_eq!(listed.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(listed.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v.as_array().expect("arr").len(), 0);
    }

    fn req(method: &str, uri: &str, body: &str, bearer: Option<&str>) -> Request<Body> {
        let mut b =
            Request::builder().method(method).uri(uri).header("content-type", "application/json");
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    async fn org_app() -> (Router, String) {
        let svc = OrganizationService::new(Arc::new(InMemoryOrgRepository::new()));
        let sessions = Arc::new(InMemorySessionStore::new());
        let router = org_router(svc, sessions.clone());
        let admin = sessions
            .create(
                "u-admin",
                PermissionSet::from_raw(["ORGANIZATION:READ+ADD+UPDATE+DELETE"]).expect("p"),
                3600,
            )
            .await
            .expect("sess");
        (router, admin)
    }

    #[tokio::test]
    async fn org_create_requires_auth() {
        let (app, _) = org_app().await;
        let r =
            app.oneshot(req("POST", "/organization", r#"{"name":"Acme"}"#, None)).await.expect("r");
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn org_create_requires_permission() {
        let svc = OrganizationService::new(Arc::new(InMemoryOrgRepository::new()));
        let sessions = Arc::new(InMemorySessionStore::new());
        let app = org_router(svc, sessions.clone());
        let viewer = sessions
            .create("u", PermissionSet::from_raw(["ORGANIZATION:READ"]).expect("p"), 3600)
            .await
            .expect("s");
        let r = app
            .oneshot(req("POST", "/organization", r#"{"name":"Acme"}"#, Some(&viewer)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn org_full_crud_flow() {
        let (app, t) = org_app().await;
        let r = app
            .clone()
            .oneshot(req("POST", "/organization", r#"{"name":"Acme"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.expect("b");
        let id = serde_json::from_slice::<serde_json::Value>(&bytes).expect("j")["id"]
            .as_str()
            .expect("id")
            .to_string();

        assert_eq!(
            app.clone()
                .oneshot(req("GET", "/organization?current=1&pageSize=10", "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req(
                    "PUT",
                    &format!("/organization/{id}"),
                    r#"{"name":"AcmeCorp","enable":false}"#,
                    Some(&t)
                ))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req("DELETE", &format!("/organization/{id}"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            app.oneshot(req("GET", &format!("/organization/{id}"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    async fn role_app() -> (Router, String) {
        let roles = Arc::new(InMemoryRoleRepository::new());
        let user_roles = Arc::new(InMemoryUserRoleRepository::new(roles.clone()));
        let sessions = Arc::new(InMemorySessionStore::new());
        let app = role_router(
            RoleService::new(roles.clone()),
            UserRoleService::new(roles, user_roles),
            sessions.clone(),
        );
        let admin = sessions
            .create(
                "u-admin",
                PermissionSet::from_raw(["USER_ROLE:READ+ADD+UPDATE+DELETE"]).expect("p"),
                3600,
            )
            .await
            .expect("s");
        (app, admin)
    }

    #[tokio::test]
    async fn role_crud_and_grant_flow() {
        let (app, t) = role_app().await;
        let r = app
            .clone()
            .oneshot(req(
                "POST",
                "/role",
                r#"{"name":"auditor","scope":"SYSTEM","permissions":["SYSTEM_USER:READ"]}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.expect("b");
        let id = serde_json::from_slice::<serde_json::Value>(&bytes).expect("j")["id"]
            .as_str()
            .expect("id")
            .to_string();
        assert_eq!(
            app.clone()
                .oneshot(req("GET", "/role?current=1&pageSize=10", "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req("GET", &format!("/role/{id}"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req(
                    "PUT",
                    &format!("/role/{id}"),
                    r#"{"name":"auditor2","permissions":["SYSTEM_USER:READ+ADD"]}"#,
                    Some(&t)
                ))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req(
                    "POST",
                    "/user-role/grant",
                    &format!(r#"{{"userId":"u-x","roleId":"{id}"}}"#),
                    Some(&t)
                ))
                .await
                .expect("r")
                .status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            app.clone()
                .oneshot(req(
                    "POST",
                    "/user-role/grant",
                    r#"{"userId":"u-x","roleId":"ghost"}"#,
                    Some(&t)
                ))
                .await
                .expect("r")
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            app.oneshot(req("DELETE", &format!("/role/{id}"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::NO_CONTENT
        );
    }

    #[tokio::test]
    async fn role_create_requires_permission() {
        let roles = Arc::new(InMemoryRoleRepository::new());
        let user_roles = Arc::new(InMemoryUserRoleRepository::new(roles.clone()));
        let sessions = Arc::new(InMemorySessionStore::new());
        let app = role_router(
            RoleService::new(roles.clone()),
            UserRoleService::new(roles, user_roles),
            sessions.clone(),
        );
        let viewer = sessions
            .create("u", PermissionSet::from_raw(["USER_ROLE:READ"]).expect("p"), 3600)
            .await
            .expect("s");
        let r = app
            .oneshot(req(
                "POST",
                "/role",
                r#"{"name":"x","scope":"SYSTEM","permissions":[]}"#,
                Some(&viewer),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn permissions_flow_from_granted_role_to_session() {
        let user_repo = Arc::new(InMemoryUserRepository::new());
        let create = CreateUserUseCase::new(user_repo.clone());
        let admin = UserService::new(user_repo);
        let resolve = ResolveUserNamesUseCase::new(Arc::new(SpyDirectory::new()));
        let creds = Arc::new(InMemoryCredentialRepository::new().with_user(
            "bob",
            "u-bob",
            "pw",
            Vec::<String>::new(),
        ));
        let hasher = Arc::new(PlainPasswordHasher);
        let sessions = Arc::new(InMemorySessionStore::new());
        let roles = Arc::new(InMemoryRoleRepository::new());
        let user_roles = Arc::new(InMemoryUserRoleRepository::new(roles.clone()));
        let role = RoleService::new(roles.clone())
            .create("creator", crate::domain::RoleScope::System, vec!["SYSTEM_USER:ADD".into()])
            .await
            .expect("role");
        UserRoleService::new(roles.clone(), user_roles.clone())
            .grant("u-bob", &role.id)
            .await
            .expect("grant");
        let login =
            LoginUseCase::new(creds.clone(), hasher.clone(), sessions.clone(), user_roles.clone());
        let app = router(create, resolve, login, admin, creds, hasher, user_roles, sessions, 3600);
        let tok = token(&app, "bob", "pw").await.expect("token");
        let r = app.oneshot(post("/system/user", create_body(), Some(&tok))).await.expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn user_admin_crud_flow() {
        let (app, _store) = app_store();
        let t = token(&app, "admin", "secret").await.expect("token");
        let r = app
            .clone()
            .oneshot(post("/system/user", r#"{"name":"Alice","email":"al@x.com"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.expect("b");
        let id = serde_json::from_slice::<serde_json::Value>(&bytes).expect("j")["id"]
            .as_str()
            .expect("id")
            .to_string();
        assert_eq!(
            app.clone()
                .oneshot(req("GET", "/system/user?current=1&pageSize=10", "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req("GET", &format!("/system/user/{id}"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req(
                    "PUT",
                    &format!("/system/user/{id}"),
                    r#"{"name":"Alice2","email":"al2@x.com","enable":false}"#,
                    Some(&t)
                ))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req("DELETE", &format!("/system/user/{id}"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            app.oneshot(req("GET", &format!("/system/user/{id}"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn list_users_requires_permission() {
        let (app, _s) = app_store();
        let t = token(&app, "viewer", "pw").await.expect("token");
        assert_eq!(
            app.clone()
                .oneshot(req("GET", "/system/user?current=1&pageSize=10", "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.oneshot(req("DELETE", "/system/user/whatever", "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::FORBIDDEN
        );
    }
}
