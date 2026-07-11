use std::sync::Arc;

use axum::{
    extract::{FromRef, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use crate::application::{FollowService, FollowServiceError};

#[derive(Clone)]
struct FollowState {
    svc: FollowService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<FollowState> for Arc<dyn SessionStore> {
    fn from_ref(s: &FollowState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(svc: FollowService, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/follow", post(follow).delete(unfollow).get(status))
        .route("/follow/mine", get(mine))
        .with_state(FollowState { svc, sessions })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FollowBody {
    project_id: String,
    entity_type: String,
    entity_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FollowStatus {
    entity_type: String,
    entity_id: String,
    following: bool,
    followers: Vec<String>,
    follower_count: usize,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct StatusQuery {
    project_id: String,
    entity_type: String,
    entity_id: String,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct MineQuery {
    project_id: String,
    entity_type: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct MineResponse {
    entity_ids: Vec<String>,
}

fn svc_err(e: FollowServiceError) -> Response {
    match e {
        FollowServiceError::Validation(_) => {
            (StatusCode::BAD_REQUEST, "invalid follow payload").into_response()
        }
        FollowServiceError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

async fn status_payload(
    st: &FollowState,
    user_id: &str,
    project_id: &str,
    entity_type: &str,
    entity_id: &str,
) -> Result<FollowStatus, FollowServiceError> {
    let followers = st.svc.followers(project_id, entity_type, entity_id).await?;
    let following = followers.iter().any(|u| u == user_id);
    Ok(FollowStatus {
        entity_type: entity_type.trim().to_ascii_lowercase(),
        entity_id: entity_id.trim().to_string(),
        following,
        follower_count: followers.len(),
        followers,
    })
}

#[utoipa::path(post, path = "/follow", tag = "follow", request_body = FollowBody, responses((status = 200, body = FollowStatus), (status = 400), (status = 401)), security(("bearer" = [])))]
async fn follow(
    user: AuthUser,
    State(st): State<FollowState>,
    Json(b): Json<FollowBody>,
) -> Response {
    if let Err(e) = st.svc.follow(&b.project_id, &b.entity_type, &b.entity_id, &user.user_id).await
    {
        return svc_err(e);
    }
    match status_payload(&st, &user.user_id, &b.project_id, &b.entity_type, &b.entity_id).await {
        Ok(s) => (StatusCode::OK, Json(s)).into_response(),
        Err(e) => svc_err(e),
    }
}

#[utoipa::path(delete, path = "/follow", tag = "follow", request_body = FollowBody, responses((status = 200, body = FollowStatus), (status = 400), (status = 401)), security(("bearer" = [])))]
async fn unfollow(
    user: AuthUser,
    State(st): State<FollowState>,
    Json(b): Json<FollowBody>,
) -> Response {
    if let Err(e) =
        st.svc.unfollow(&b.project_id, &b.entity_type, &b.entity_id, &user.user_id).await
    {
        return svc_err(e);
    }
    match status_payload(&st, &user.user_id, &b.project_id, &b.entity_type, &b.entity_id).await {
        Ok(s) => (StatusCode::OK, Json(s)).into_response(),
        Err(e) => svc_err(e),
    }
}

#[utoipa::path(get, path = "/follow", tag = "follow", params(StatusQuery), responses((status = 200, body = FollowStatus), (status = 400), (status = 401)), security(("bearer" = [])))]
async fn status(
    user: AuthUser,
    State(st): State<FollowState>,
    Query(q): Query<StatusQuery>,
) -> Response {
    match status_payload(&st, &user.user_id, &q.project_id, &q.entity_type, &q.entity_id).await {
        Ok(s) => (StatusCode::OK, Json(s)).into_response(),
        Err(e) => svc_err(e),
    }
}

#[utoipa::path(get, path = "/follow/mine", tag = "follow", params(MineQuery), responses((status = 200, body = MineResponse), (status = 400), (status = 401)), security(("bearer" = [])))]
async fn mine(
    user: AuthUser,
    State(st): State<FollowState>,
    Query(q): Query<MineQuery>,
) -> Response {
    match st.svc.following_ids(&q.project_id, &user.user_id, q.entity_type.as_deref()).await {
        Ok(entity_ids) => (StatusCode::OK, Json(MineResponse { entity_ids })).into_response(),
        Err(e) => svc_err(e),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(follow, unfollow, status, mine),
    components(schemas(FollowBody, FollowStatus, MineResponse)),
    tags((name = "follow", description = "关注人(通用)"))
)]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryFollowStore;
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app() -> (Router, String) {
        let store = Arc::new(InMemoryFollowStore::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["FOLLOW:READ".to_string()]).expect("perms");
        let token = sessions.create("alice", perms, 3600).await.expect("token");
        let r = router(FollowService::new(store), sessions);
        (r, token)
    }

    fn req(method: &str, uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b =
            Request::builder().method(method).uri(uri).header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    #[tokio::test]
    async fn follow_then_status_then_unfollow() {
        let (app, t) = app().await;
        let payload = r#"{"projectId":"p1","entityType":"bug","entityId":"b1"}"#;

        let r = app.clone().oneshot(req("POST", "/follow", payload, Some(&t))).await.expect("r");
        assert_eq!(r.status(), StatusCode::OK);
        let v = body_json(r).await;
        assert_eq!(v["following"], true);
        assert_eq!(v["followerCount"], 1);
        assert_eq!(v["followers"][0], "alice");

        let m = app
            .clone()
            .oneshot(req("GET", "/follow/mine?projectId=p1&entityType=bug", "", Some(&t)))
            .await
            .expect("r");
        assert_eq!(body_json(m).await["entityIds"][0], "b1");

        let s = app
            .clone()
            .oneshot(req("GET", "/follow?projectId=p1&entityType=bug&entityId=b1", "", Some(&t)))
            .await
            .expect("r");
        assert_eq!(body_json(s).await["following"], true);

        let d = app.clone().oneshot(req("DELETE", "/follow", payload, Some(&t))).await.expect("r");
        let dv = body_json(d).await;
        assert_eq!(dv["following"], false);
        assert_eq!(dv["followerCount"], 0);
    }

    #[tokio::test]
    async fn follow_without_token_401() {
        let (app, _t) = app().await;
        let r = app
            .oneshot(req(
                "POST",
                "/follow",
                r#"{"projectId":"p1","entityType":"bug","entityId":"b1"}"#,
                None,
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn follow_blank_entity_400() {
        let (app, t) = app().await;
        let r = app
            .oneshot(req(
                "POST",
                "/follow",
                r#"{"projectId":"p1","entityType":"bug","entityId":""}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }
}
