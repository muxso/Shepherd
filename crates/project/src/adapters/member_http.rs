use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete as delete_route, get},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use crate::application::{MemberCmdError, ProjectMemberService};
use crate::domain::ProjectMember;

#[derive(Clone)]
struct MemberState {
    members: ProjectMemberService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<MemberState> for Arc<dyn SessionStore> {
    fn from_ref(s: &MemberState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(members: ProjectMemberService, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/project/{id}/member", get(list_members).post(add_member))
        .route("/project/{id}/member/{userId}", delete_route(remove_member))
        .with_state(MemberState { members, sessions })
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct MemberResponse {
    project_id: String,
    user_id: String,
    role: String,
    added_at: String,
}

impl From<ProjectMember> for MemberResponse {
    fn from(m: ProjectMember) -> Self {
        Self {
            project_id: m.project_id,
            user_id: m.user_id,
            role: m.role.as_str().to_string(),
            added_at: m.added_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct AddMemberRequest {
    user_id: String,
    #[serde(default = "default_role")]
    role: String,
}

fn default_role() -> String {
    "MEMBER".to_string()
}

#[utoipa::path(
    post, path = "/project/{id}/member", tag = "project",
    params(("id" = String, Path)),
    request_body = AddMemberRequest,
    responses((status = 201, body = MemberResponse), (status = 400), (status = 403)),
    security(("bearer" = []))
)]
async fn add_member(
    user: AuthUser,
    State(st): State<MemberState>,
    Path(id): Path<String>,
    Json(req): Json<AddMemberRequest>,
) -> Response {
    if !user.can("PROJECT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.members.add(&id, &req.user_id, &req.role).await {
        Ok(m) => (StatusCode::CREATED, Json(MemberResponse::from(m))).into_response(),
        Err(MemberCmdError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid member payload").into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(
    get, path = "/project/{id}/member", tag = "project",
    params(("id" = String, Path)),
    responses((status = 200, body = [MemberResponse]), (status = 403)),
    security(("bearer" = []))
)]
async fn list_members(
    user: AuthUser,
    State(st): State<MemberState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("PROJECT", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.members.list(&id).await {
        Ok(list) => {
            let out: Vec<MemberResponse> = list.into_iter().map(MemberResponse::from).collect();
            (StatusCode::OK, Json(out)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(
    delete, path = "/project/{id}/member/{userId}", tag = "project",
    params(("id" = String, Path), ("userId" = String, Path)),
    responses((status = 204), (status = 403), (status = 404)),
    security(("bearer" = []))
)]
async fn remove_member(
    user: AuthUser,
    State(st): State<MemberState>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response {
    if !user.can("PROJECT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.members.remove(&id, &user_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(MemberCmdError::NotFound) => {
            (StatusCode::NOT_FOUND, "member not found").into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(add_member, list_members, remove_member),
    components(schemas(AddMemberRequest, MemberResponse)),
    tags((name = "project", description = "项目管理"))
)]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryProjectMemberRepository;
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app_with(perm: &str) -> (Router, String) {
        let repo = Arc::new(InMemoryProjectMemberRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw([perm.to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        (router(ProjectMemberService::new(repo), sessions), token)
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
    async fn add_then_list_member() {
        let (app, t) = app_with("PROJECT:READ+ADD+UPDATE").await;
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/project/p1/member",
                Some(r#"{"userId":"u1","role":"owner"}"#),
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = body_json(resp).await;
        assert_eq!(v["role"], "OWNER");

        let resp =
            app.oneshot(json_req("GET", "/project/p1/member", None, Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v.as_array().expect("arr").len(), 1);
        assert_eq!(v[0]["userId"], "u1");
    }

    #[tokio::test]
    async fn add_defaults_role_to_member() {
        let (app, t) = app_with("PROJECT:READ+UPDATE").await;
        let resp = app
            .oneshot(json_req("POST", "/project/p1/member", Some(r#"{"userId":"u1"}"#), Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(body_json(resp).await["role"], "MEMBER");
    }

    #[tokio::test]
    async fn add_invalid_role_400() {
        let (app, t) = app_with("PROJECT:UPDATE").await;
        let resp = app
            .oneshot(json_req(
                "POST",
                "/project/p1/member",
                Some(r#"{"userId":"u1","role":"king"}"#),
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn add_without_update_permission_403() {
        let (app, t) = app_with("PROJECT:READ").await;
        let resp = app
            .oneshot(json_req("POST", "/project/p1/member", Some(r#"{"userId":"u1"}"#), Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn add_without_token_401() {
        let (app, _t) = app_with("PROJECT:UPDATE").await;
        let resp = app
            .oneshot(json_req("POST", "/project/p1/member", Some(r#"{"userId":"u1"}"#), None))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn remove_then_404() {
        let (app, t) = app_with("PROJECT:READ+UPDATE").await;
        app.clone()
            .oneshot(json_req("POST", "/project/p1/member", Some(r#"{"userId":"u1"}"#), Some(&t)))
            .await
            .expect("seed");
        let resp = app
            .clone()
            .oneshot(json_req("DELETE", "/project/p1/member/u1", None, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let resp = app
            .oneshot(json_req("DELETE", "/project/p1/member/u1", None, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
