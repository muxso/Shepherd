//! 缺陷管理的 HTTP 适配器:`POST /bug`(创建)、`POST /bug/{id}/status`(变更状态)。
//!
//! 状态机规则全在 domain/application;本层只做 DTO 翻译 + 错误码映射。
//! 注意映射:非法流转 → 409 Conflict(状态冲突),未知状态 → 400,缺陷不存在 → 404。
//! 两个写端点经 `webauth::AuthUser` 做 RBAC:创建需 `BUG:ADD`,改状态需 `BUG:UPDATE`。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, post},
    Json, Router,
};
use crate::application::{
    BugFollowerError, BugFollowersUseCase, ChangeBugStatusError, ChangeBugStatusUseCase,
    CreateBugError, CreateBugUseCase, ListBugsUseCase,
};
use crate::domain::{Bug, BugError};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct BugState {
    create: CreateBugUseCase,
    change: ChangeBugStatusUseCase,
    list: ListBugsUseCase,
    followers: BugFollowersUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<BugState> for Arc<dyn SessionStore> {
    fn from_ref(s: &BugState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    create: CreateBugUseCase,
    change: ChangeBugStatusUseCase,
    list: ListBugsUseCase,
    followers: BugFollowersUseCase,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/bug", post(create_bug).get(list_bugs))
        .route("/bug/{id}/status", post(change_status))
        .route("/bug/{id}/followers", post(follow_bug).get(list_followers))
        .route("/bug/{id}/followers/{userId}", delete(unfollow_bug))
        .with_state(BugState { create, change, list, followers, sessions })
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct BugResponse {
    id: String,
    project_id: String,
    title: String,
    status: String,
    created_at: i64,
    /// 创建人 user_id(历史行为 null)。
    #[serde(skip_serializing_if = "Option::is_none")]
    created_by: Option<String>,
}

impl From<Bug> for BugResponse {
    fn from(b: Bug) -> Self {
        Self {
            id: b.id,
            project_id: b.project_id,
            title: b.title,
            status: b.status,
            created_at: b.created_at,
            created_by: b.created_by,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CreateBugRequest {
    project_id: String,
    title: String,
    initial_status: String,
}

#[utoipa::path(post, path = "/bug", tag = "bug", request_body = CreateBugRequest, responses((status = 201, body = BugResponse), (status = 400)), security(("bearer" = [])))]
async fn create_bug(
    user: AuthUser,
    State(st): State<BugState>,
    Json(req): Json<CreateBugRequest>,
) -> Response {
    if !user.can("BUG", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.create.execute(&req.project_id, &req.title, &req.initial_status, Some(&user.user_id)).await {
        Ok(b) => (StatusCode::CREATED, Json(BugResponse::from(b))).into_response(),
        Err(CreateBugError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid bug payload").into_response()
        }
        Err(CreateBugError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListBugsQuery {
    project_id: String,
}

#[utoipa::path(
    get, path = "/bug", tag = "bug",
    params(("projectId" = String, Query, description = "项目 ID")),
    responses((status = 200, body = [BugResponse]), (status = 403)),
    security(("bearer" = []))
)]
async fn list_bugs(
    user: AuthUser,
    State(st): State<BugState>,
    Query(q): Query<ListBugsQuery>,
) -> Response {
    if !user.can("BUG", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.list.execute(&q.project_id).await {
        Ok(bugs) => {
            let body: Vec<BugResponse> = bugs.into_iter().map(BugResponse::from).collect();
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
struct ChangeStatusRequest {
    status: String,
}

#[utoipa::path(post, path = "/bug/{id}/status", tag = "bug", params(("id" = String, Path)), request_body = ChangeStatusRequest, responses((status = 200, body = BugResponse), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn change_status(
    user: AuthUser,
    State(st): State<BugState>,
    Path(id): Path<String>,
    Json(req): Json<ChangeStatusRequest>,
) -> Response {
    if !user.can("BUG", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.change.execute(&id, &req.status).await {
        Ok(b) => (StatusCode::OK, Json(BugResponse::from(b))).into_response(),
        Err(ChangeBugStatusError::BugNotFound) => {
            (StatusCode::NOT_FOUND, "bug not found").into_response()
        }
        Err(ChangeBugStatusError::Domain(BugError::TransitionNotAllowed { .. })) => {
            (StatusCode::CONFLICT, "transition not allowed").into_response()
        }
        Err(ChangeBugStatusError::Domain(_)) => {
            (StatusCode::BAD_REQUEST, "invalid target status").into_response()
        }
        Err(ChangeBugStatusError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

// ——— 关注人(关注 / 取消关注 / 列表)———

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FollowersResponse {
    /// 关注人 user_id 列表(按关注先后)。
    followers: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FollowRequest {
    /// 要加入关注的 user_id。
    user_id: String,
}

fn map_follower_err(e: BugFollowerError) -> Response {
    match e {
        BugFollowerError::BugNotFound => (StatusCode::NOT_FOUND, "bug not found").into_response(),
        BugFollowerError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/bug/{id}/followers", tag = "bug", params(("id" = String, Path)), responses((status = 200, body = FollowersResponse), (status = 404)), security(("bearer" = [])))]
async fn list_followers(user: AuthUser, State(st): State<BugState>, Path(id): Path<String>) -> Response {
    if !user.can("BUG", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.followers.list(&id).await {
        Ok(followers) => (StatusCode::OK, Json(FollowersResponse { followers })).into_response(),
        Err(e) => map_follower_err(e),
    }
}

#[utoipa::path(post, path = "/bug/{id}/followers", tag = "bug", params(("id" = String, Path)), request_body = FollowRequest, responses((status = 200, body = FollowersResponse), (status = 404)), security(("bearer" = [])))]
async fn follow_bug(
    user: AuthUser,
    State(st): State<BugState>,
    Path(id): Path<String>,
    Json(req): Json<FollowRequest>,
) -> Response {
    if !user.can("BUG", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.followers.follow(&id, &req.user_id).await {
        Ok(followers) => (StatusCode::OK, Json(FollowersResponse { followers })).into_response(),
        Err(e) => map_follower_err(e),
    }
}

#[utoipa::path(delete, path = "/bug/{id}/followers/{userId}", tag = "bug", params(("id" = String, Path), ("userId" = String, Path)), responses((status = 200, body = FollowersResponse), (status = 404)), security(("bearer" = [])))]
async fn unfollow_bug(
    user: AuthUser,
    State(st): State<BugState>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response {
    if !user.can("BUG", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.followers.unfollow(&id, &user_id).await {
        Ok(followers) => (StatusCode::OK, Json(FollowersResponse { followers })).into_response(),
        Err(e) => map_follower_err(e),
    }
}

#[derive(OpenApi)]
#[openapi(paths(create_bug, list_bugs, change_status, list_followers, follow_bug, unfollow_bug), components(schemas(CreateBugRequest, ChangeStatusRequest, BugResponse, FollowRequest, FollowersResponse)), tags((name = "bug", description = "缺陷管理")))]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi { ApiDoc::openapi() }

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use crate::adapters::InMemoryBugRepository;
    use kernel::permission::PermissionSet;
    use std::sync::Arc;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    /// app + 一个拥有 `BUG:READ+ADD+UPDATE` 的令牌。
    async fn app() -> (Router, String) {
        let repo = Arc::new(InMemoryBugRepository::with_default_flow("p1"));
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["BUG:READ+ADD+UPDATE".to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        let r = router(
            CreateBugUseCase::new(repo.clone()),
            ChangeBugStatusUseCase::new(repo.clone()),
            ListBugsUseCase::new(repo.clone()),
            BugFollowersUseCase::new(repo),
            sessions,
        );
        (r, token)
    }

    fn post(uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method("POST").uri(uri).header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    fn get(uri: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method("GET").uri(uri);
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::empty()).expect("req")
    }

    async fn create_returns_id(app: &Router, t: &str) -> String {
        let resp = app
            .clone()
            .oneshot(post("/bug", r#"{"projectId":"p1","title":"boom","initialStatus":"NEW"}"#, Some(t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        v["id"].as_str().expect("id").to_string()
    }

    #[tokio::test]
    async fn create_then_valid_transition_200() {
        let (app, t) = app().await;
        let id = create_returns_id(&app, &t).await;
        let resp = app
            .oneshot(post(&format!("/bug/{id}/status"), r#"{"status":"RESOLVED"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["status"], "RESOLVED");
    }

    #[tokio::test]
    async fn create_without_token_401() {
        let (app, _t) = app().await;
        let resp = app
            .oneshot(post("/bug", r#"{"projectId":"p1","title":"x","initialStatus":"NEW"}"#, None))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn change_status_without_permission_403() {
        let repo = Arc::new(InMemoryBugRepository::with_default_flow("p1"));
        let sessions = Arc::new(InMemorySessionStore::new());
        // 有 ADD 无 UPDATE
        let perms = PermissionSet::from_raw(["BUG:READ+ADD".to_string()]).expect("perms");
        let token = sessions.create("u", perms, 3600).await.expect("token");
        let app = router(
            CreateBugUseCase::new(repo.clone()),
            ChangeBugStatusUseCase::new(repo.clone()),
            ListBugsUseCase::new(repo.clone()),
            BugFollowersUseCase::new(repo),
            sessions,
        );
        let id = create_returns_id(&app, &token).await;
        let resp = app
            .oneshot(post(&format!("/bug/{id}/status"), r#"{"status":"RESOLVED"}"#, Some(&token)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn create_with_unknown_initial_status_400() {
        let (app, t) = app().await;
        let resp = app
            .oneshot(post("/bug", r#"{"projectId":"p1","title":"x","initialStatus":"GHOST"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn disallowed_transition_409() {
        let (app, t) = app().await;
        let id = create_returns_id(&app, &t).await;
        // NEW 不能直达 CLOSED
        let resp = app
            .oneshot(post(&format!("/bug/{id}/status"), r#"{"status":"CLOSED"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn change_status_of_missing_bug_404() {
        let (app, t) = app().await;
        let resp = app
            .oneshot(post("/bug/ghost/status", r#"{"status":"RESOLVED"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_returns_created_bugs_newest_first() {
        let (app, t) = app().await;
        app.clone()
            .oneshot(post("/bug", r#"{"projectId":"p1","title":"first","initialStatus":"NEW"}"#, Some(&t)))
            .await
            .expect("resp");
        app.clone()
            .oneshot(post("/bug", r#"{"projectId":"p1","title":"second","initialStatus":"NEW"}"#, Some(&t)))
            .await
            .expect("resp");

        let resp = app.oneshot(get("/bug?projectId=p1", Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let arr = v.as_array().expect("array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["title"], "second"); // 新建在前
        assert_eq!(arr[1]["title"], "first");
    }

    #[tokio::test]
    async fn list_without_permission_403() {
        let repo = Arc::new(InMemoryBugRepository::with_default_flow("p1"));
        let sessions = Arc::new(InMemorySessionStore::new());
        // 有 ADD 无 READ
        let perms = PermissionSet::from_raw(["BUG:ADD".to_string()]).expect("perms");
        let token = sessions.create("u", perms, 3600).await.expect("token");
        let app = router(
            CreateBugUseCase::new(repo.clone()),
            ChangeBugStatusUseCase::new(repo.clone()),
            ListBugsUseCase::new(repo.clone()),
            BugFollowersUseCase::new(repo),
            sessions,
        );
        let resp = app.oneshot(get("/bug?projectId=p1", Some(&token))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn transition_to_unknown_status_400() {
        let (app, t) = app().await;
        let id = create_returns_id(&app, &t).await;
        let resp = app
            .oneshot(post(&format!("/bug/{id}/status"), r#"{"status":"GHOST"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
