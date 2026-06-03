//! 缺陷管理的 HTTP 适配器:`POST /bug`(创建)、`POST /bug/{id}/status`(变更状态)。
//!
//! 状态机规则全在 domain/application;本层只做 DTO 翻译 + 错误码映射。
//! 注意映射:非法流转 → 409 Conflict(状态冲突),未知状态 → 400,缺陷不存在 → 404。
//! 两个写端点经 `webauth::AuthUser` 做 RBAC:创建需 `BUG:ADD`,改状态需 `BUG:UPDATE`。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use crate::application::{
    ChangeBugStatusError, ChangeBugStatusUseCase, CreateBugError, CreateBugUseCase,
};
use crate::domain::{Bug, BugError};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct BugState {
    create: CreateBugUseCase,
    change: ChangeBugStatusUseCase,
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
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/bug", post(create_bug))
        .route("/bug/{id}/status", post(change_status))
        .with_state(BugState { create, change, sessions })
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct BugResponse {
    id: String,
    project_id: String,
    title: String,
    status: String,
}

impl From<Bug> for BugResponse {
    fn from(b: Bug) -> Self {
        Self { id: b.id, project_id: b.project_id, title: b.title, status: b.status }
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
    match st.create.execute(&req.project_id, &req.title, &req.initial_status).await {
        Ok(b) => (StatusCode::CREATED, Json(BugResponse::from(b))).into_response(),
        Err(CreateBugError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid bug payload").into_response()
        }
        Err(CreateBugError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
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

#[derive(OpenApi)]
#[openapi(paths(create_bug, change_status), components(schemas(CreateBugRequest, ChangeStatusRequest, BugResponse)), tags((name = "bug", description = "缺陷管理")))]
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
            ChangeBugStatusUseCase::new(repo),
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
            ChangeBugStatusUseCase::new(repo),
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
