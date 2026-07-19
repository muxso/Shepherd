use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use crate::application::NoticeQueryService;
use crate::domain::Notice;
use crate::ports::{ListQuery, Tab};

#[derive(Clone)]
struct NoticeState {
    query: NoticeQueryService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<NoticeState> for Arc<dyn SessionStore> {
    fn from_ref(s: &NoticeState) -> Self {
        s.sessions.clone()
    }
}

// Personal inbox: any authenticated user, always scoped to their own receiver_id.
pub fn router(query: NoticeQueryService, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/notice", get(list_notices))
        .route("/notice/unread-count", get(unread_count))
        .route("/notice/read-all", post(read_all))
        .route("/notice/{id}/read", post(mark_read))
        .with_state(NoticeState { query, sessions })
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct NoticeResponse {
    id: String,
    project_id: String,
    category: String,
    event_type: String,
    title: String,
    content: String,
    resource_type: String,
    resource_id: String,
    operator: String,
    at_mention: bool,
    read: bool,
    /// Epoch millis.
    created_at: i64,
}

impl From<Notice> for NoticeResponse {
    fn from(n: Notice) -> Self {
        Self {
            id: n.id,
            project_id: n.project_id,
            category: n.category,
            event_type: n.event_type,
            title: n.title,
            content: n.content,
            resource_type: n.resource_type,
            resource_id: n.resource_id,
            operator: n.operator,
            at_mention: n.at_mention,
            read: n.read,
            created_at: n.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct NoticePageResponse {
    items: Vec<NoticeResponse>,
    total: u64,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct ListNoticesQuery {
    project_id: Option<String>,
    /// PLAN | BUG | CASE | API | SCHEDULE.
    category: Option<String>,
    /// all | at | unread | read (default all).
    tab: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

#[utoipa::path(get, path = "/notice", tag = "notice", params(ListNoticesQuery), responses((status = 200, body = NoticePageResponse), (status = 400)), security(("bearer" = [])))]
async fn list_notices(
    user: AuthUser,
    State(st): State<NoticeState>,
    Query(q): Query<ListNoticesQuery>,
) -> Response {
    let Some(tab) = Tab::parse(q.tab.as_deref().unwrap_or("all")) else {
        return (StatusCode::BAD_REQUEST, "unknown tab").into_response();
    };
    let query = ListQuery {
        receiver_id: user.user_id,
        project_id: q.project_id.filter(|p| !p.trim().is_empty()),
        category: q.category.filter(|c| !c.trim().is_empty()),
        tab,
        page: q.page.unwrap_or(1),
        page_size: q.page_size.unwrap_or(50),
    };
    match st.query.list(query).await {
        Ok(page) => (
            StatusCode::OK,
            Json(NoticePageResponse {
                items: page.items.into_iter().map(NoticeResponse::from).collect(),
                total: page.total,
            }),
        )
            .into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct ProjectQuery {
    project_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct UnreadCountResponse {
    total: u64,
    by_category: BTreeMap<String, u64>,
}

#[utoipa::path(get, path = "/notice/unread-count", tag = "notice", params(ProjectQuery), responses((status = 200, body = UnreadCountResponse)), security(("bearer" = [])))]
async fn unread_count(
    user: AuthUser,
    State(st): State<NoticeState>,
    Query(q): Query<ProjectQuery>,
) -> Response {
    let project = q.project_id.filter(|p| !p.trim().is_empty());
    match st.query.unread_count(&user.user_id, project.as_deref()).await {
        Ok(c) => (
            StatusCode::OK,
            Json(UnreadCountResponse {
                total: c.total,
                by_category: c.by_category.into_iter().collect(),
            }),
        )
            .into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(post, path = "/notice/{id}/read", tag = "notice", params(("id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn mark_read(
    user: AuthUser,
    State(st): State<NoticeState>,
    Path(id): Path<String>,
) -> Response {
    match st.query.mark_read(&id, &user.user_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "notice not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(post, path = "/notice/read-all", tag = "notice", params(ProjectQuery), responses((status = 204)), security(("bearer" = [])))]
async fn read_all(
    user: AuthUser,
    State(st): State<NoticeState>,
    Query(q): Query<ProjectQuery>,
) -> Response {
    let project = q.project_id.filter(|p| !p.trim().is_empty());
    match st.query.mark_all_read(&user.user_id, project.as_deref()).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(paths(list_notices, unread_count, mark_read, read_all), components(schemas(NoticeResponse, NoticePageResponse, UnreadCountResponse)), tags((name = "notice", description = "站内信")))]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::in_memory::{InMemoryNoticeStore, InMemoryUserDirectory};
    use crate::application::{NoticeEvent, Notifier};
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app() -> (Router, String, Notifier) {
        let store = Arc::new(InMemoryNoticeStore::new());
        let dir = Arc::new(InMemoryUserDirectory::new().with_user("alice", &["alice"]));
        let notifier = Notifier::new(store.clone(), dir);
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["PROJECT:READ".to_string()]).expect("perms");
        let token = sessions.create("alice", perms, 3600).await.expect("token");
        (router(NoticeQueryService::new(store), sessions), token, notifier)
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

    fn event(category: &str, at: bool) -> (Vec<String>, NoticeEvent, bool) {
        (
            vec!["alice".to_string()],
            NoticeEvent {
                project_id: "p1".into(),
                category: category.into(),
                event_type: "BUG_ASSIGNED".into(),
                title: "登录页崩溃".into(),
                content: "NEW".into(),
                resource_type: "BUG".into(),
                resource_id: "b1".into(),
                operator: "admin".into(),
            },
            at,
        )
    }

    async fn seed(n: &Notifier, category: &str, at: bool) {
        let (receivers, ev, at_mention) = event(category, at);
        if at_mention {
            assert_eq!(n.notify_mentions("@alice", ev).await, 1);
        } else {
            assert_eq!(n.notify(receivers, ev).await, 1);
        }
    }

    #[tokio::test]
    async fn list_scopes_to_current_user_and_filters_tabs() {
        let (app, t, notifier) = app().await;
        seed(&notifier, "BUG", false).await;
        seed(&notifier, "CASE", true).await;

        let resp =
            app.clone().oneshot(req("GET", "/notice?projectId=p1", Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json(resp).await;
        assert_eq!(v["total"], 2);

        let resp = app.clone().oneshot(req("GET", "/notice?tab=at", Some(&t))).await.expect("resp");
        let v = json(resp).await;
        assert_eq!(v["total"], 1);
        assert_eq!(v["items"][0]["atMention"], true);

        let resp =
            app.clone().oneshot(req("GET", "/notice?category=BUG", Some(&t))).await.expect("resp");
        assert_eq!(json(resp).await["total"], 1);

        // Unknown tab is a 400, and no token a 401.
        let resp =
            app.clone().oneshot(req("GET", "/notice?tab=ghost", Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let resp = app.oneshot(req("GET", "/notice", None)).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unread_count_and_read_flow() {
        let (app, t, notifier) = app().await;
        seed(&notifier, "BUG", false).await;
        seed(&notifier, "BUG", false).await;
        seed(&notifier, "CASE", true).await;

        let resp = app
            .clone()
            .oneshot(req("GET", "/notice/unread-count?projectId=p1", Some(&t)))
            .await
            .expect("resp");
        let v = json(resp).await;
        assert_eq!(v["total"], 3);
        assert_eq!(v["byCategory"]["BUG"], 2);
        assert_eq!(v["byCategory"]["CASE"], 1);

        // Mark one read via its id from the list.
        let resp =
            app.clone().oneshot(req("GET", "/notice?category=CASE", Some(&t))).await.expect("resp");
        let id = json(resp).await["items"][0]["id"].as_str().expect("id").to_string();
        let resp = app
            .clone()
            .oneshot(req("POST", &format!("/notice/{id}/read"), Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        // Marking twice still succeeds; a ghost id is a 404.
        let resp =
            app.clone().oneshot(req("POST", "/notice/ghost/read", Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let resp = app
            .clone()
            .oneshot(req("POST", "/notice/read-all?projectId=p1", Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let resp = app.oneshot(req("GET", "/notice/unread-count", Some(&t))).await.expect("resp");
        assert_eq!(json(resp).await["total"], 0);
    }
}
