use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete as delete_route, post},
    Json, Router,
};

use crate::application::{
    AddCommentError, AddCommentUseCase, DeleteCommentError, DeleteCommentUseCase,
    ListCommentsUseCase,
};
use crate::domain::Comment;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct CommentState {
    add: AddCommentUseCase,
    list: ListCommentsUseCase,
    delete: DeleteCommentUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<CommentState> for Arc<dyn SessionStore> {
    fn from_ref(s: &CommentState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    add: AddCommentUseCase,
    list: ListCommentsUseCase,
    delete: DeleteCommentUseCase,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/comment", post(add_comment).get(list_comments))
        .route("/comment/{id}", delete_route(delete_comment))
        .with_state(CommentState { add, list, delete, sessions })
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CommentResponse {
    id: String,
    target_type: String,
    target_id: String,
    content: String,
    author: String,
    created_at: String,
}

impl From<Comment> for CommentResponse {
    fn from(c: Comment) -> Self {
        Self {
            id: c.id,
            target_type: c.target_type,
            target_id: c.target_id,
            content: c.content,
            author: c.author,
            created_at: c.created_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct AddCommentRequest {
    target_type: String,
    target_id: String,
    content: String,
}

#[utoipa::path(post, path = "/comment", tag = "comment", request_body = AddCommentRequest, responses((status = 201, body = CommentResponse), (status = 400)), security(("bearer" = [])))]
async fn add_comment(
    user: AuthUser,
    State(st): State<CommentState>,
    Json(req): Json<AddCommentRequest>,
) -> Response {
    if !user.can("COMMENT", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.add.execute(&req.target_type, &req.target_id, &req.content, &user.user_id).await {
        Ok(c) => (StatusCode::CREATED, Json(CommentResponse::from(c))).into_response(),
        Err(AddCommentError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid comment payload").into_response()
        }
        Err(AddCommentError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    target_type: String,
    target_id: String,
}

#[utoipa::path(get, path = "/comment", tag = "comment", params(ListQuery), responses((status = 200, body = [CommentResponse])), security(("bearer" = [])))]
async fn list_comments(
    user: AuthUser,
    State(st): State<CommentState>,
    Query(q): Query<ListQuery>,
) -> Response {
    if !user.can("COMMENT", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.list.execute(&q.target_type, &q.target_id).await {
        Ok(list) => {
            let out: Vec<CommentResponse> = list.into_iter().map(CommentResponse::from).collect();
            (StatusCode::OK, Json(out)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(delete, path = "/comment/{id}", tag = "comment", params(("id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn delete_comment(
    user: AuthUser,
    State(st): State<CommentState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("COMMENT", "DELETE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.delete.execute(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(DeleteCommentError::NotFound) => {
            (StatusCode::NOT_FOUND, "comment not found").into_response()
        }
        Err(DeleteCommentError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(OpenApi)]
#[openapi(paths(add_comment, list_comments, delete_comment), components(schemas(AddCommentRequest, CommentResponse)), tags((name = "comment", description = "通用评论")))]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryCommentRepository;
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app() -> (Router, String) {
        let repo = Arc::new(InMemoryCommentRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["COMMENT:READ+ADD+DELETE".to_string()]).expect("perms");
        let token = sessions.create("alice", perms, 3600).await.expect("token");
        let r = router(
            AddCommentUseCase::new(repo.clone()),
            ListCommentsUseCase::new(repo.clone()),
            DeleteCommentUseCase::new(repo),
            sessions,
        );
        (r, token)
    }

    fn post(uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    fn req(method: &str, uri: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::empty()).expect("req")
    }

    async fn json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    async fn add_returns_id(app: &Router, t: &str) -> String {
        let resp = app
            .clone()
            .oneshot(post(
                "/comment",
                r#"{"targetType":"BUG","targetId":"b1","content":"复现步骤"}"#,
                Some(t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = json(resp).await;
        assert_eq!(v["author"], "alice");
        v["id"].as_str().expect("id").to_string()
    }

    #[tokio::test]
    async fn add_then_list_returns_comment() {
        let (app, t) = app().await;
        add_returns_id(&app, &t).await;
        let resp = app
            .oneshot(req("GET", "/comment?targetType=BUG&targetId=b1", Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json(resp).await;
        assert_eq!(v.as_array().expect("arr").len(), 1);
        assert_eq!(v[0]["content"], "复现步骤");
    }

    #[tokio::test]
    async fn add_without_token_401() {
        let (app, _t) = app().await;
        let resp = app
            .oneshot(post(
                "/comment",
                r#"{"targetType":"BUG","targetId":"b1","content":"x"}"#,
                None,
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn add_blank_content_400() {
        let (app, t) = app().await;
        let resp = app
            .oneshot(post(
                "/comment",
                r#"{"targetType":"BUG","targetId":"b1","content":"   "}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn add_without_permission_403() {
        let repo = Arc::new(InMemoryCommentRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["COMMENT:READ".to_string()]).expect("perms");
        let token = sessions.create("bob", perms, 3600).await.expect("token");
        let app = router(
            AddCommentUseCase::new(repo.clone()),
            ListCommentsUseCase::new(repo.clone()),
            DeleteCommentUseCase::new(repo),
            sessions,
        );
        let resp = app
            .oneshot(post(
                "/comment",
                r#"{"targetType":"BUG","targetId":"b1","content":"x"}"#,
                Some(&token),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_then_gone() {
        let (app, t) = app().await;
        let id = add_returns_id(&app, &t).await;
        let resp = app
            .clone()
            .oneshot(req("DELETE", &format!("/comment/{id}"), Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let resp = app
            .oneshot(req("GET", "/comment?targetType=BUG&targetId=b1", Some(&t)))
            .await
            .expect("resp");
        let v = json(resp).await;
        assert_eq!(v.as_array().expect("arr").len(), 0);
    }

    #[tokio::test]
    async fn delete_missing_404() {
        let (app, t) = app().await;
        let resp = app
            .oneshot(req("DELETE", "/comment/ghost", Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
