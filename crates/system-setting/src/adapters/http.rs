//! HTTP 适配器(feature = "http")。DTO ↔ 用例,领域错误映射 HTTP 码。
//! 含鉴权:`POST /auth/login` 发令牌;`AuthUser` 提取器从 Bearer 令牌还原用户+权限;
//! 写操作用 `kernel` 权限做 RBAC(无令牌→401,权限不足→403)。

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
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
use kernel::page::PageRequest;
use crate::domain::{AuthError, AuthUser, OidcError};
use crate::ports::{CredentialRepository, PasswordHasher, SessionStore, UserRoleQuery};

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
}

// 让 AuthUser 提取器能从 AppState 取到 SessionStore
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
) -> Router {
    Router::new()
        .route("/auth/login", post(login_handler)) // 公开
        .route("/auth/logout", post(logout_handler)) // 撤销当前令牌
        // 静态段 /names 在 axum 中优先于 /{id}
        .route("/system/user/names", get(resolve_names))
        .route("/system/user", post(create_user).get(list_users))
        .route("/system/user/{id}", get(get_user).put(update_user).delete(delete_user))
        .route("/system/user/{id}/reset-password", post(reset_password))
        .with_state(AppState { create, resolve, login, admin, creds, hasher, user_roles, sessions })
}

// ---- 登录 ----
#[derive(Deserialize, ToSchema)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize, ToSchema)]
struct LoginResponse {
    token: String,
}

#[utoipa::path(post, path = "/auth/login", tag = "auth", request_body = LoginRequest, responses((status = 200, body = LoginResponse), (status = 401)))]
async fn login_handler(State(st): State<AppState>, Json(req): Json<LoginRequest>) -> Response {
    match st.login.execute(&req.username, &req.password).await {
        Ok(token) => (StatusCode::OK, Json(LoginResponse { token })).into_response(),
        Err(AuthError::InvalidCredentials) => {
            (StatusCode::UNAUTHORIZED, "invalid credentials").into_response()
        }
        Err(AuthError::Backend(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "auth error").into_response()
        }
    }
}

/// 登出:撤销 Authorization 头里的令牌。幂等,总返回 204。
#[utoipa::path(post, path = "/auth/logout", tag = "auth", responses((status = 204)), security(("bearer" = [])))]
async fn logout_handler(State(st): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
    {
        let _ = st.sessions.revoke(token).await;
    }
    StatusCode::NO_CONTENT.into_response()
}

// ---- 建用户(受保护) ----
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
    /// 用户组(角色名);列表接口附带,其余接口为空。
    #[serde(default)]
    user_groups: Vec<String>,
}
impl From<crate::domain::User> for UserResponse {
    fn from(u: crate::domain::User) -> Self {
        Self { id: u.id, name: u.name, email: u.email.as_str().to_string(), enable: u.enable, user_groups: Vec::new() }
    }
}

fn user_err_response(e: UserCmdError) -> Response {
    match e {
        UserCmdError::Validation(_) => (StatusCode::BAD_REQUEST, "invalid user").into_response(),
        UserCmdError::EmailExists => (StatusCode::CONFLICT, "email already exists").into_response(),
        UserCmdError::NotFound => (StatusCode::NOT_FOUND, "user not found").into_response(),
        UserCmdError::Repo(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
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
async fn list_users(user: AuthUser, State(st): State<AppState>, Query(q): Query<OrgListQuery>) -> Response {
    if !user.can("SYSTEM_USER", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let page = match PageRequest::new(q.current, q.page_size) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid page params").into_response(),
    };
    match st.admin.list(page).await {
        Ok(p) => {
            let (total, current, page_size, total_pages) = (p.total, p.current, p.page_size, p.total_pages());
            // 附带用户组(角色名):批量查 ms_user_role,按 user_id 归集。
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
    /// 重置后的新密码(明文,供管理员转交用户)。
    password: String,
}

#[utoipa::path(post, path = "/system/user/{id}/reset-password", tag = "user", params(("id" = String, Path)), responses((status = 200, body = ResetPwResponse), (status = 404)), security(("bearer" = [])))]
async fn reset_password(user: AuthUser, State(st): State<AppState>, Path(id): Path<String>) -> Response {
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
        Ok(()) => (StatusCode::OK, Json(ResetPwResponse { password: new_pw.to_string() })).into_response(),
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
async fn delete_user(user: AuthUser, State(st): State<AppState>, Path(id): Path<String>) -> Response {
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
    user: AuthUser, // 401 if missing/invalid token(提取器保证)
    State(st): State<AppState>,
    Json(req): Json<CreateUserRequest>,
) -> Response {
    // RBAC:需要 SYSTEM_USER:ADD
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

#[utoipa::path(get, path = "/system/user/names", tag = "user", params(NamesQuery), responses((status = 200)))]
async fn resolve_names(State(st): State<AppState>, Query(q): Query<NamesQuery>) -> Response {
    let ids: Vec<String> = q
        .ids
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let names: BTreeMap<String, String> = st.resolve.execute(&ids).await;
    (StatusCode::OK, Json(names)).into_response()
}

// ---- 第三方登录路由(飞书/企业微信);独立 router,组装根 merge 进来 ----
pub fn oidc_router(oidc: OidcLoginUseCase) -> Router {
    Router::new()
        .route("/auth/oidc/{provider}/authorize", get(oidc_authorize))
        .route("/auth/oidc/{provider}/callback", get(oidc_callback))
        .with_state(oidc)
}

#[derive(Deserialize, IntoParams)]
struct AuthorizeQuery {
    #[serde(default)]
    state: String,
}

#[utoipa::path(get, path = "/auth/oidc/{provider}/authorize", tag = "auth", params(("provider" = String, Path), AuthorizeQuery), responses((status = 302), (status = 404)))]
async fn oidc_authorize(
    State(uc): State<OidcLoginUseCase>,
    Path(provider): Path<String>,
    Query(q): Query<AuthorizeQuery>,
) -> Response {
    match uc.authorize_url(&provider, &q.state) {
        Ok(url) => Redirect::to(&url).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "unknown provider").into_response(),
    }
}

#[derive(Deserialize, IntoParams)]
struct CallbackQuery {
    code: String,
    #[serde(default)]
    #[allow(dead_code)]
    state: String,
}

#[utoipa::path(get, path = "/auth/oidc/{provider}/callback", tag = "auth", params(("provider" = String, Path), CallbackQuery), responses((status = 200), (status = 401)))]
async fn oidc_callback(
    State(uc): State<OidcLoginUseCase>,
    Path(provider): Path<String>,
    Query(q): Query<CallbackQuery>,
) -> Response {
    // 生产应校验 state(CSRF),此处从略。
    match uc.complete(&provider, &q.code).await {
        Ok(token) => (StatusCode::OK, Json(LoginResponse { token })).into_response(),
        Err(OidcError::UnknownProvider(_)) => {
            (StatusCode::NOT_FOUND, "unknown provider").into_response()
        }
        Err(OidcError::Exchange(_)) => {
            (StatusCode::UNAUTHORIZED, "identity exchange failed").into_response()
        }
        Err(OidcError::Backend(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "auth error").into_response()
        }
    }
}

// ---- 组织 CRUD 路由(RBAC:ORGANIZATION:READ/ADD/UPDATE/DELETE)----
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
        OrgCmdError::Validation(_) => (StatusCode::BAD_REQUEST, "invalid organization").into_response(),
        OrgCmdError::NameExists => (StatusCode::CONFLICT, "name already exists").into_response(),
        OrgCmdError::NotFound => (StatusCode::NOT_FOUND, "organization not found").into_response(),
        OrgCmdError::Repo(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
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
async fn create_org(user: AuthUser, State(st): State<OrgState>, Json(req): Json<OrgBody>) -> Response {
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
async fn list_orgs(user: AuthUser, State(st): State<OrgState>, Query(q): Query<OrgListQuery>) -> Response {
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
async fn delete_org(user: AuthUser, State(st): State<OrgState>, Path(id): Path<String>) -> Response {
    if !user.can("ORGANIZATION", "DELETE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.svc.delete(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => org_err_response(e),
    }
}

// ---- 角色 CRUD + 用户-角色授权路由(RBAC:USER_ROLE:READ/ADD/UPDATE/DELETE)----
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
        Self { id: r.id, name: r.name, scope: r.scope.as_str().to_string(), permissions: r.permissions }
    }
}

fn role_err_response(e: RoleCmdError) -> Response {
    match e {
        RoleCmdError::Validation(_) => (StatusCode::BAD_REQUEST, "invalid role").into_response(),
        RoleCmdError::NotFound => (StatusCode::NOT_FOUND, "role not found").into_response(),
        RoleCmdError::Repo(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
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
async fn create_role(user: AuthUser, State(st): State<RoleState>, Json(req): Json<RoleBody>) -> Response {
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
async fn list_roles(user: AuthUser, State(st): State<RoleState>, Query(q): Query<OrgListQuery>) -> Response {
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
async fn delete_role(user: AuthUser, State(st): State<RoleState>, Path(id): Path<String>) -> Response {
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
async fn grant_role(user: AuthUser, State(st): State<RoleState>, Json(req): Json<GrantBody>) -> Response {
    if !user.can("USER_ROLE", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.grants.grant(&req.user_id, &req.role_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => role_err_response(e),
    }
}

#[utoipa::path(post, path = "/user-role/revoke", tag = "role", request_body = GrantBody, responses((status = 200)), security(("bearer" = [])))]
async fn revoke_role(user: AuthUser, State(st): State<RoleState>, Json(req): Json<GrantBody>) -> Response {
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
        login_handler, logout_handler, resolve_names, oidc_authorize, oidc_callback,
        create_user, list_users, get_user, update_user, delete_user,
        create_org, list_orgs, get_org, update_org, delete_org,
        create_role, list_roles, get_role, update_role, delete_role, grant_role, revoke_role
    ),
    components(schemas(LoginRequest, LoginResponse, CreateUserRequest, UserResponse, UserPage, UpdateUserBody, OrgResponse, OrgBody, OrgPage, RoleResponse, RoleBody, RolePage, GrantBody)),
    tags((name = "auth", description = "鉴权"), (name = "user", description = "用户管理"), (name = "organization", description = "组织"), (name = "role", description = "角色 / 授权"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{
        FakeIdentityProvider, InMemoryCredentialRepository, InMemoryExternalUserRepository,
        InMemoryOrgRepository, InMemoryRoleRepository, InMemorySessionStore, InMemoryUserRepository,
        InMemoryUserRoleRepository, PlainPasswordHasher, SpyDirectory,
    };
    use crate::domain::ExternalIdentity;
    use kernel::permission::PermissionSet;
    use axum::body::Body;
    use axum::http::Request;
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
        ); // 只有 READ
        let hasher = Arc::new(PlainPasswordHasher);
        let sessions = Arc::new(InMemorySessionStore::new());
        let user_roles = Arc::new(InMemoryUserRoleRepository::new(Arc::new(InMemoryRoleRepository::new())));
        let login = LoginUseCase::new(creds.clone(), hasher.clone(), sessions.clone(), user_roles.clone());
        (
            router(create, resolve, login, admin, creds, hasher, user_roles, sessions.clone()),
            sessions,
        )
    }

    fn app() -> Router {
        app_store().0
    }

    fn post(uri: &str, body: &str, bearer: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method("POST").uri(uri).header("content-type", "application/json");
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    async fn token(app: &Router, user: &str, pass: &str) -> Option<String> {
        let resp = app
            .clone()
            .oneshot(post("/auth/login", &format!(r#"{{"username":"{user}","password":"{pass}"}}"#), None))
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
        assert!(token(&app, "admin", "WRONG").await.is_none()); // 401
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
        let t = token(&app, "viewer", "pw").await.expect("token"); // 只有 READ
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
        // 登出前可用
        assert_eq!(
            app.clone().oneshot(post("/system/user", create_body(), Some(&t))).await.expect("r").status(),
            StatusCode::CREATED
        );
        // 登出
        let lo = app.clone().oneshot(post("/auth/logout", "", Some(&t))).await.expect("r");
        assert_eq!(lo.status(), StatusCode::NO_CONTENT);
        // 登出后令牌失效 → 401
        assert_eq!(
            app.oneshot(post("/system/user", create_body(), Some(&t))).await.expect("r").status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn expired_session_is_401() {
        let (app, store) = app_store();
        store.insert_expired("expired-tok", "u-admin"); // 注入一个已过期会话
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

    #[tokio::test]
    async fn oidc_authorize_redirects() {
        let resp = oidc_app().oneshot(get_req("/auth/oidc/feishu/authorize?state=s1")).await.expect("r");
        assert!(resp.status().is_redirection());
        let loc = resp.headers().get("location").expect("loc").to_str().expect("s");
        assert!(loc.contains("fake.example") && loc.contains("state=s1"));
    }

    #[tokio::test]
    async fn oidc_authorize_unknown_provider_404() {
        let resp = oidc_app().oneshot(get_req("/auth/oidc/github/authorize")).await.expect("r");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn oidc_callback_good_code_returns_token() {
        let resp = oidc_app().oneshot(get_req("/auth/oidc/feishu/callback?code=ok&state=s")).await.expect("r");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(v["token"].as_str().expect("tok").starts_with("sess-"));
    }

    #[tokio::test]
    async fn oidc_callback_bad_code_401() {
        let resp = oidc_app().oneshot(get_req("/auth/oidc/feishu/callback?code=bad")).await.expect("r");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ---- 组织 CRUD ----
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
        let r = app.oneshot(req("POST", "/organization", r#"{"name":"Acme"}"#, None)).await.expect("r");
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
        assert_eq!(r.status(), StatusCode::FORBIDDEN); // 只有 READ,无 ADD
    }

    #[tokio::test]
    async fn org_full_crud_flow() {
        let (app, t) = org_app().await;
        let r = app.clone().oneshot(req("POST", "/organization", r#"{"name":"Acme"}"#, Some(&t))).await.expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.expect("b");
        let id = serde_json::from_slice::<serde_json::Value>(&bytes).expect("j")["id"]
            .as_str()
            .expect("id")
            .to_string();

        assert_eq!(
            app.clone().oneshot(req("GET", "/organization?current=1&pageSize=10", "", Some(&t))).await.expect("r").status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone().oneshot(req("PUT", &format!("/organization/{id}"), r#"{"name":"AcmeCorp","enable":false}"#, Some(&t))).await.expect("r").status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone().oneshot(req("DELETE", &format!("/organization/{id}"), "", Some(&t))).await.expect("r").status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            app.oneshot(req("GET", &format!("/organization/{id}"), "", Some(&t))).await.expect("r").status(),
            StatusCode::NOT_FOUND
        );
    }

    // ---- 角色 CRUD + 授权 ----
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
        // create
        let r = app
            .clone()
            .oneshot(req("POST", "/role", r#"{"name":"auditor","scope":"SYSTEM","permissions":["SYSTEM_USER:READ"]}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.expect("b");
        let id = serde_json::from_slice::<serde_json::Value>(&bytes).expect("j")["id"]
            .as_str()
            .expect("id")
            .to_string();
        // list / get / update
        assert_eq!(app.clone().oneshot(req("GET", "/role?current=1&pageSize=10", "", Some(&t))).await.expect("r").status(), StatusCode::OK);
        assert_eq!(app.clone().oneshot(req("GET", &format!("/role/{id}"), "", Some(&t))).await.expect("r").status(), StatusCode::OK);
        assert_eq!(app.clone().oneshot(req("PUT", &format!("/role/{id}"), r#"{"name":"auditor2","permissions":["SYSTEM_USER:READ+ADD"]}"#, Some(&t))).await.expect("r").status(), StatusCode::OK);
        // grant 已存在角色 → 204
        assert_eq!(app.clone().oneshot(req("POST", "/user-role/grant", &format!(r#"{{"userId":"u-x","roleId":"{id}"}}"#), Some(&t))).await.expect("r").status(), StatusCode::NO_CONTENT);
        // grant 不存在角色 → 404
        assert_eq!(app.clone().oneshot(req("POST", "/user-role/grant", r#"{"userId":"u-x","roleId":"ghost"}"#, Some(&t))).await.expect("r").status(), StatusCode::NOT_FOUND);
        // delete
        assert_eq!(app.oneshot(req("DELETE", &format!("/role/{id}"), "", Some(&t))).await.expect("r").status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn role_create_requires_permission() {
        let roles = Arc::new(InMemoryRoleRepository::new());
        let user_roles = Arc::new(InMemoryUserRoleRepository::new(roles.clone()));
        let sessions = Arc::new(InMemorySessionStore::new());
        let app = role_router(RoleService::new(roles.clone()), UserRoleService::new(roles, user_roles), sessions.clone());
        let viewer = sessions.create("u", PermissionSet::from_raw(["USER_ROLE:READ"]).expect("p"), 3600).await.expect("s");
        let r = app.oneshot(req("POST", "/role", r#"{"name":"x","scope":"SYSTEM","permissions":[]}"#, Some(&viewer))).await.expect("r");
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
    }

    // 闭环:用户凭证本身无权限,靠被授予的角色拿到 SYSTEM_USER:ADD,登录后即可建用户。
    #[tokio::test]
    async fn permissions_flow_from_granted_role_to_session() {
        let user_repo = Arc::new(InMemoryUserRepository::new());
        let create = CreateUserUseCase::new(user_repo.clone());
        let admin = UserService::new(user_repo);
        let resolve = ResolveUserNamesUseCase::new(Arc::new(SpyDirectory::new()));
        let creds = Arc::new(
            InMemoryCredentialRepository::new().with_user("bob", "u-bob", "pw", Vec::<String>::new()),
        );
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
        let login = LoginUseCase::new(creds.clone(), hasher.clone(), sessions.clone(), user_roles.clone());
        let app = router(create, resolve, login, admin, creds, hasher, user_roles, sessions);
        let tok = token(&app, "bob", "pw").await.expect("token");
        // bob 凭证无权限,但角色授予了 SYSTEM_USER:ADD → 建用户 201
        let r = app.oneshot(post("/system/user", create_body(), Some(&tok))).await.expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn user_admin_crud_flow() {
        let (app, _store) = app_store();
        let t = token(&app, "admin", "secret").await.expect("token"); // admin 有 READ+ADD+UPDATE+DELETE
        // create
        let r = app.clone().oneshot(post("/system/user", r#"{"name":"Alice","email":"al@x.com"}"#, Some(&t))).await.expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.expect("b");
        let id = serde_json::from_slice::<serde_json::Value>(&bytes).expect("j")["id"].as_str().expect("id").to_string();
        // list / get / update / delete / get-404
        assert_eq!(app.clone().oneshot(req("GET", "/system/user?current=1&pageSize=10", "", Some(&t))).await.expect("r").status(), StatusCode::OK);
        assert_eq!(app.clone().oneshot(req("GET", &format!("/system/user/{id}"), "", Some(&t))).await.expect("r").status(), StatusCode::OK);
        assert_eq!(app.clone().oneshot(req("PUT", &format!("/system/user/{id}"), r#"{"name":"Alice2","email":"al2@x.com","enable":false}"#, Some(&t))).await.expect("r").status(), StatusCode::OK);
        assert_eq!(app.clone().oneshot(req("DELETE", &format!("/system/user/{id}"), "", Some(&t))).await.expect("r").status(), StatusCode::NO_CONTENT);
        assert_eq!(app.oneshot(req("GET", &format!("/system/user/{id}"), "", Some(&t))).await.expect("r").status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_users_requires_permission() {
        let (app, _s) = app_store();
        let t = token(&app, "viewer", "pw").await.expect("token"); // 只有 READ
        // READ 可列表
        assert_eq!(app.clone().oneshot(req("GET", "/system/user?current=1&pageSize=10", "", Some(&t))).await.expect("r").status(), StatusCode::OK);
        // 但无 DELETE 权限 → 403
        assert_eq!(app.oneshot(req("DELETE", "/system/user/whatever", "", Some(&t))).await.expect("r").status(), StatusCode::FORBIDDEN);
    }
}
