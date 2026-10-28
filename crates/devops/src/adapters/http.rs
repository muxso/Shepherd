use crate::{
    application::Service,
    domain::{CompleteRun, Error, ListQuery, NewSource, Record},
};
use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};
#[derive(Clone)]
struct AppState {
    svc: Service,
    sessions: Arc<dyn SessionStore>,
}
impl FromRef<AppState> for Arc<dyn SessionStore> {
    fn from_ref(s: &AppState) -> Self {
        s.sessions.clone()
    }
}
pub fn router(svc: Service, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/api/devops/sources", get(sources).post(create_source))
        .route("/api/devops/sources/{id}/runs", post(start))
        .route("/api/devops/sources/{id}/dashboard", get(dashboard))
        .route("/api/devops/sources/{id}/records", get(records))
        .route("/api/devops/runs/{id}/records", put(stage))
        .route("/api/devops/runs/{id}/complete", post(complete))
        .route("/api/devops/runs/{id}/fail", post(fail))
        .with_state(AppState { svc, sessions })
}
fn reply(v: Result<Value, Error>) -> Response {
    match v {
        Ok(v) => Json(v).into_response(),
        Err(e) => {
            let status = match e {
                Error::Invalid(_) => StatusCode::BAD_REQUEST,
                Error::NotFound => StatusCode::NOT_FOUND,
                Error::Conflict(_) => StatusCode::CONFLICT,
                Error::Storage => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, e.to_string()).into_response()
        }
    }
}
fn denied() -> Response {
    (StatusCode::FORBIDDEN, "permission denied").into_response()
}
fn can_sync(u: &AuthUser) -> bool {
    u.can("DEVOPS", "SYNC") || u.can("PROJECT", "UPDATE")
}
#[utoipa::path(get,path="/api/devops/sources",tag="devops",responses((status=200,body=Value),(status=401),(status=403)),security(("bearer"=[])))]
async fn sources(u: AuthUser, State(s): State<AppState>) -> Response {
    if !u.can("PROJECT", "READ") {
        return denied();
    }
    reply(s.svc.0.sources().await)
}
#[utoipa::path(post,path="/api/devops/sources",tag="devops",request_body=NewSource,responses((status=200,body=Value),(status=400),(status=403)),security(("bearer"=[])))]
async fn create_source(
    u: AuthUser,
    State(s): State<AppState>,
    Json(b): Json<NewSource>,
) -> Response {
    if !u.can("PROJECT", "UPDATE") {
        return denied();
    }
    if let Err(e) = b.validate() {
        return reply(Err(e));
    }
    reply(s.svc.0.create_source(b).await)
}
#[utoipa::path(post,path="/api/devops/sources/{id}/runs",tag="devops",params(("id"=String,Path)),responses((status=200,body=Value),(status=409),(status=403)),security(("bearer"=[])))]
async fn start(u: AuthUser, State(s): State<AppState>, Path(id): Path<String>) -> Response {
    if !can_sync(&u) {
        return denied();
    }
    reply(s.svc.0.start(&id).await)
}
#[derive(Deserialize, ToSchema)]
struct Batch {
    records: Vec<Record>,
}
#[utoipa::path(put,path="/api/devops/runs/{id}/records",tag="devops",params(("id"=String,Path)),request_body=Batch,responses((status=200,body=Value),(status=400),(status=403)),security(("bearer"=[])))]
async fn stage(
    u: AuthUser,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<Batch>,
) -> Response {
    if !can_sync(&u) {
        return denied();
    }
    reply(s.svc.stage(&id, b.records).await)
}
#[utoipa::path(post,path="/api/devops/runs/{id}/complete",tag="devops",params(("id"=String,Path)),request_body=CompleteRun,responses((status=200,body=Value),(status=409),(status=403)),security(("bearer"=[])))]
async fn complete(
    u: AuthUser,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<CompleteRun>,
) -> Response {
    if !can_sync(&u) {
        return denied();
    }
    if let Err(e) = b.validate() {
        return reply(Err(e));
    }
    reply(s.svc.0.complete(&id, b).await)
}
#[derive(Deserialize, ToSchema)]
struct Failure {
    error: String,
}
#[utoipa::path(post,path="/api/devops/runs/{id}/fail",tag="devops",params(("id"=String,Path)),request_body=Failure,responses((status=200,body=Value),(status=403)),security(("bearer"=[])))]
async fn fail(
    u: AuthUser,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<Failure>,
) -> Response {
    if !can_sync(&u) {
        return denied();
    }
    reply(s.svc.0.fail(&id, &b.error).await)
}
#[utoipa::path(get,path="/api/devops/sources/{id}/dashboard",tag="devops",params(("id"=String,Path),ListQuery),responses((status=200,body=Value),(status=403)),security(("bearer"=[])))]
async fn dashboard(
    u: AuthUser,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ListQuery>,
) -> Response {
    if !u.can("PROJECT", "READ") || !u.can("REQUIREMENT", "READ") || !u.can("BUG", "READ") {
        return denied();
    }
    reply(s.svc.0.dashboard(&id, q.days.unwrap_or(7).clamp(1, 366)).await)
}
#[utoipa::path(get,path="/api/devops/sources/{id}/records",tag="devops",params(("id"=String,Path),ListQuery),responses((status=200,body=Value),(status=403)),security(("bearer"=[])))]
async fn records(
    u: AuthUser,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ListQuery>,
) -> Response {
    let resource = match q.kind.as_deref().unwrap_or("requirement") {
        "requirement" => "REQUIREMENT",
        "bug" => "BUG",
        "testcase" => "FUNCTIONAL_CASE",
        _ => "PROJECT",
    };
    if !u.can("PROJECT", "READ")
        || !u.can(resource, "READ")
        || (q.kind.as_deref() == Some("history")
            && (!u.can("BUG", "READ") || !u.can("REQUIREMENT", "READ")))
    {
        return denied();
    }
    reply(s.svc.0.records(&id, q).await)
}
#[derive(OpenApi)]
#[openapi(paths(sources,create_source,start,stage,complete,fail,dashboard,records),components(schemas(NewSource,Record,CompleteRun,Batch,Failure)),tags((name="devops",description="External DevOps facts and synchronization")))]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Store;
    use async_trait::async_trait;
    use axum::{body::Body, http::Request};
    use serde_json::json;
    use tower::ServiceExt;
    use webauth::{testing::InMemorySessionStore, PermissionSet};
    struct Stub;
    #[async_trait]
    impl Store for Stub {
        async fn sources(&self) -> Result<Value, Error> {
            Ok(json!({"items":[]}))
        }
        async fn create_source(&self, _: NewSource) -> Result<Value, Error> {
            Ok(json!({}))
        }
        async fn start(&self, _: &str) -> Result<Value, Error> {
            Ok(json!({"id":"run"}))
        }
        async fn stage(&self, _: &str, _: Vec<Record>) -> Result<Value, Error> {
            Ok(json!({}))
        }
        async fn complete(&self, _: &str, _: CompleteRun) -> Result<Value, Error> {
            Ok(json!({}))
        }
        async fn fail(&self, _: &str, _: &str) -> Result<Value, Error> {
            Ok(json!({}))
        }
        async fn dashboard(&self, _: &str, _: i64) -> Result<Value, Error> {
            Ok(json!({}))
        }
        async fn records(&self, _: &str, _: ListQuery) -> Result<Value, Error> {
            Ok(json!({}))
        }
    }
    #[tokio::test]
    async fn auth_and_sync_permissions_are_enforced() {
        let sessions = Arc::new(InMemorySessionStore::new());
        let read = sessions
            .create("reader", PermissionSet::from_raw(["PROJECT:READ"]).expect("perms"), 3600)
            .await
            .expect("session");
        let sync = sessions
            .create("sync", PermissionSet::from_raw(["DEVOPS:SYNC"]).expect("perms"), 3600)
            .await
            .expect("session");
        let app = router(Service(Arc::new(Stub)), sessions);
        for (method, path, token, expected) in [
            ("GET", "/api/devops/sources", None, 401),
            ("GET", "/api/devops/sources", Some(read.as_str()), 200),
            ("POST", "/api/devops/sources/source/runs", Some(read.as_str()), 403),
            ("POST", "/api/devops/sources/source/runs", Some(sync.as_str()), 200),
            ("GET", "/api/devops/sources/source/dashboard", Some(read.as_str()), 403),
            ("GET", "/api/devops/sources/source/records?kind=bug", Some(read.as_str()), 403),
        ] {
            let mut r = Request::builder().method(method).uri(path);
            if let Some(t) = token {
                r = r.header("Authorization", format!("Bearer {t}"));
            }
            let response = app
                .clone()
                .oneshot(r.body(Body::empty()).expect("request"))
                .await
                .expect("response");
            assert_eq!(response.status().as_u16(), expected, "{method} {path}");
        }
    }
}
