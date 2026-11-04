use crate::domain::code_authors::CodeAuthorsQuery;
use crate::{
    application::Service,
    domain::{
        CompleteRun, Error, ListQuery, MemberMetricsQuery, NewSource, ProductivityMetric, Record,
        RequirementOutputQuery,
    },
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
        .route("/api/devops/requirement-output", get(requirement_output))
        .route("/api/devops/sources/{id}/member-metrics", get(member_metrics))
        .route("/api/devops/sources/{id}/code-authors", get(code_authors))
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
    if !u.can("PROJECT", "READ")
        || !u.can("REQUIREMENT", "READ")
        || !u.can("BUG", "READ")
        || !u.can("FUNCTIONAL_CASE", "READ")
    {
        return denied();
    }
    reply(s.svc.0.dashboard(&id, q.days.unwrap_or(7).clamp(1, 366)).await)
}
#[utoipa::path(get,path="/api/devops/sources/{id}/code-authors",tag="devops",params(("id"=String,Path),CodeAuthorsQuery),responses((status=200,body=Value),(status=400),(status=401),(status=403),(status=404)),security(("bearer"=[])))]
async fn code_authors(
    u: AuthUser,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<CodeAuthorsQuery>,
) -> Response {
    if !u.can("PROJECT", "READ") {
        return denied();
    }
    if let Err(error) = q.bounds() {
        return reply(Err(error));
    }
    let can = u.can("REQUIREMENT", "READ");
    let result = s.svc.0.code_authors(&id, q, can).await.map(|mut data| {
        data["permissions"] = serde_json::json!({"requirements":can});
        if !can {
            if let Some(commits) = data["commits"].as_array_mut() {
                for commit in commits {
                    commit["requirements"] = serde_json::json!([]);
                }
            }
        }
        data
    });
    reply(result)
}
#[utoipa::path(get,path="/api/devops/sources/{id}/member-metrics",tag="devops",params(("id"=String,Path),MemberMetricsQuery),responses((status=200,body=Value),(status=400),(status=401),(status=403),(status=404)),security(("bearer"=[])))]
async fn member_metrics(
    u: AuthUser,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<MemberMetricsQuery>,
) -> Response {
    if !u.can("PROJECT", "READ") || !u.can("REQUIREMENT", "READ") || !u.can("BUG", "READ") {
        return denied();
    }
    if let Err(error) = q.validate() {
        return reply(Err(error));
    }
    reply(s.svc.0.member_metrics(&id, q).await)
}
#[utoipa::path(get,path="/api/devops/requirement-output",tag="devops",params(RequirementOutputQuery),responses((status=200,body=Value),(status=400),(status=403),(status=404),(status=409)),security(("bearer"=[])))]
async fn requirement_output(
    u: AuthUser,
    State(s): State<AppState>,
    Query(q): Query<RequirementOutputQuery>,
) -> Response {
    if !u.can("PROJECT", "READ") || !u.can("REQUIREMENT", "READ") {
        return denied();
    }
    let result = s.svc.0.requirement_output(q).await.map(|mut result| {
        let can_bugs = u.can("BUG", "READ");
        if !can_bugs {
            if let Some(output) = result["output"].as_object_mut() {
                for field in ["bugs", "bugTotal", "bugStates", "testers"] {
                    output.remove(field);
                }
            }
        }
        result["permissions"] = serde_json::json!({"bugs":can_bugs});
        result
    });
    reply(result)
}
#[utoipa::path(get,path="/api/devops/sources/{id}/records",tag="devops",params(("id"=String,Path),ListQuery),responses((status=200,body=Value),(status=403)),security(("bearer"=[])))]
async fn records(
    u: AuthUser,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ListQuery>,
) -> Response {
    let resource = match q
        .metric
        .map(|metric| metric.record_kind())
        .or(q.kind.as_deref())
        .unwrap_or("requirement")
    {
        "requirement" => "REQUIREMENT",
        "bug" => "BUG",
        "testcase" => "FUNCTIONAL_CASE",
        _ => "PROJECT",
    };
    if !u.can("PROJECT", "READ")
        || !u.can(resource, "READ")
        || (q.metric.is_some_and(|metric| metric.record_kind() == "commit")
            && !u.can("REQUIREMENT", "READ"))
        || (q.kind.as_deref() == Some("history")
            && (!u.can("BUG", "READ") || !u.can("REQUIREMENT", "READ")))
    {
        return denied();
    }
    reply(s.svc.0.records(&id, q).await.map(|mut result| {
        if !u.can("REQUIREMENT", "READ") {
            if let Some(items) = result["items"].as_array_mut() {
                for item in items {
                    if let Some(row) = item.as_object_mut() {
                        row.remove("requirements");
                    }
                }
            }
        }
        result
    }))
}
#[derive(OpenApi)]
#[openapi(paths(sources,create_source,start,stage,complete,fail,dashboard,requirement_output,member_metrics,code_authors,records),components(schemas(NewSource,Record,CompleteRun,Batch,Failure,ProductivityMetric)),tags((name="devops",description="External DevOps facts and synchronization")))]
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
        async fn requirement_output(&self, _: RequirementOutputQuery) -> Result<Value, Error> {
            Ok(
                json!({"output":{"bugs":[{"title":"private bug"}],"bugTotal":1,"testers":[{"name":"private tester"}],"bugStates":[],"commitTotal":2}}),
            )
        }
        async fn code_authors(
            &self,
            _: &str,
            _: CodeAuthorsQuery,
            can: bool,
        ) -> Result<Value, Error> {
            Ok(
                json!({"permissions":{"requirements":can},"commits":[{"requirements":[{"title":"private requirement"}]}]}),
            )
        }
        async fn member_metrics(&self, _: &str, _: MemberMetricsQuery) -> Result<Value, Error> {
            Ok(json!({"metrics":{"members":[]}}))
        }
        async fn records(&self, _: &str, _: ListQuery) -> Result<Value, Error> {
            Ok(json!({}))
        }
    }
    #[tokio::test]
    async fn requirement_output_hides_bug_details_without_bug_read_permission() {
        let sessions = Arc::new(InMemorySessionStore::new());
        let token = sessions
            .create(
                "product",
                PermissionSet::from_raw(["PROJECT:READ", "REQUIREMENT:READ"]).expect("permissions"),
                3600,
            )
            .await
            .expect("session");
        let app = router(Service(Arc::new(Stub)), sessions);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/devops/requirement-output?projectId=p&reference=REQ-42")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1_000_000).await.expect("body");
        let data: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(data["permissions"]["bugs"], false);
        assert!(data["output"].get("bugs").is_none());
        assert!(data["output"].get("testers").is_none());
        assert!(data["output"].get("bugTotal").is_none());
        assert_eq!(data["output"]["commitTotal"], 2);
    }

    #[tokio::test]
    async fn code_authors_redacts_requirement_names_at_http_boundary() {
        let sessions = Arc::new(InMemorySessionStore::new());
        let token = sessions
            .create("reader", PermissionSet::from_raw(["PROJECT:READ"]).expect("perms"), 3600)
            .await
            .expect("session");
        let response = router(Service(Arc::new(Stub)), sessions)
            .oneshot(
                Request::builder()
                    .uri("/api/devops/sources/source/code-authors?month=2026-11")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1_000_000).await.expect("body");
        let data: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(data["permissions"]["requirements"], false);
        assert_eq!(data["commits"][0]["requirements"], json!([]));
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
        let metrics = sessions
            .create(
                "metrics",
                PermissionSet::from_raw(["PROJECT:READ", "REQUIREMENT:READ", "BUG:READ"])
                    .expect("perms"),
                3600,
            )
            .await
            .expect("session");
        let app = router(Service(Arc::new(Stub)), sessions);
        for (method, path, token, expected) in [
            ("GET", "/api/devops/sources/source/code-authors?month=2026-11", None, 401),
            ("GET", "/api/devops/sources/source/code-authors?month=2026-11", Some(sync.as_str()), 403),
            ("GET", "/api/devops/sources/source/code-authors?month=2026-13", Some(read.as_str()), 400),
            ("GET", "/api/devops/sources/source/code-authors?month=2026-11", Some(read.as_str()), 200),
            ("GET", "/api/devops/sources/source/member-metrics?month=2026-10",Some(metrics.as_str()),200),
            ("GET", "/api/devops/sources/source/member-metrics?month=2026-13",Some(metrics.as_str()),400),
            ("GET", "/api/devops/sources/source/member-metrics?requirementTarget=0",Some(metrics.as_str()),400),
            ("GET", "/api/devops/sources", None, 401),
            ("GET", "/api/devops/sources/source/member-metrics?month=2026-10", None, 401),
            ("GET", "/api/devops/sources/source/member-metrics?month=2026-10", Some(read.as_str()), 403),
            ("GET", "/api/devops/requirement-output?projectId=p&reference=REQ-42", None, 401),
            ("GET", "/api/devops/requirement-output?projectId=p&reference=REQ-42", Some(read.as_str()), 403),
            ("GET", "/api/devops/sources", Some(read.as_str()), 200),
            ("POST", "/api/devops/sources/source/runs", Some(read.as_str()), 403),
            ("POST", "/api/devops/sources/source/runs", Some(sync.as_str()), 200),
            ("GET", "/api/devops/sources/source/dashboard", Some(read.as_str()), 403),
            ("GET", "/api/devops/sources/source/records?kind=bug", Some(read.as_str()), 403),
            ("GET", "/api/devops/sources/source/records?metric=linkedCommits&date=2026-11-01&kind=commit", Some(read.as_str()), 403),
            ("GET", "/api/devops/sources/source/records?metric=testcases&date=2026-11-01&kind=requirement", Some(read.as_str()), 403),
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
