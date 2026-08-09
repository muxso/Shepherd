use std::collections::BTreeMap;
use std::sync::Arc;

use crate::application::{
    BugCustomFieldsError, BugCustomFieldsUseCase, BugFollowerError, BugFollowersUseCase,
    BugRelationError, BugRelationsUseCase, ChangeBugStatusError, ChangeBugStatusUseCase,
    CreateBugError, CreateBugUseCase, ListBugsUseCase, UpdateBugMetaError, UpdateBugMetaUseCase,
};
use crate::domain::{Bug, BugError, BugRelation};
use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use notice::application::{NoticeEvent, Notifier};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct BugState {
    create: CreateBugUseCase,
    change: ChangeBugStatusUseCase,
    list: ListBugsUseCase,
    followers: BugFollowersUseCase,
    relations: BugRelationsUseCase,
    custom_fields: BugCustomFieldsUseCase,
    update: UpdateBugMetaUseCase,
    notifier: Option<Notifier>,
    sessions: Arc<dyn SessionStore>,
}

/// In-app message to the bug's handler (assignment / status change); best-effort.
async fn notify_handler(st: &BugState, bug: &Bug, event_type: &str, content: &str, operator: &str) {
    let (Some(notifier), Some(handler)) = (&st.notifier, &bug.handler) else { return };
    notifier
        .notify(
            vec![handler.clone()],
            NoticeEvent {
                project_id: bug.project_id.clone(),
                category: "BUG".into(),
                event_type: event_type.into(),
                title: bug.title.clone(),
                content: content.into(),
                resource_type: "BUG".into(),
                resource_id: bug.id.clone(),
                operator: operator.into(),
            },
        )
        .await;
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
    relations: BugRelationsUseCase,
    notifier: Option<Notifier>,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    // Built here from the create use case's repository so callers' signatures stay unchanged.
    let custom_fields = BugCustomFieldsUseCase::new(create.repo());
    let update = UpdateBugMetaUseCase::new(create.repo());
    Router::new()
        .route("/bug", post(create_bug).get(list_bugs))
        .route("/bug/{id}", put(update_bug))
        .route("/bug/{id}/status", post(change_status))
        .route("/bug/{id}/custom-fields", put(set_custom_fields))
        .route("/bug/{id}/followers", post(follow_bug).get(list_followers))
        .route("/bug/{id}/followers/{userId}", delete(unfollow_bug))
        .route("/bug/{id}/relation", post(link_relation).get(list_relations))
        .route("/bug/{id}/relation/{kind}/{targetId}", delete(unlink_relation))
        .route("/bug/by-plan/{planId}", get(list_bugs_by_plan))
        .with_state(BugState {
            create,
            change,
            list,
            followers,
            relations,
            custom_fields,
            update,
            notifier,
            sessions,
        })
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct BugResponse {
    id: String,
    project_id: String,
    title: String,
    status: String,
    created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_by: Option<String>,
    /// Severity level P0..P3 (P0 highest).
    #[serde(skip_serializing_if = "Option::is_none")]
    severity: Option<String>,
    /// Handler user id.
    #[serde(skip_serializing_if = "Option::is_none")]
    handler: Option<String>,
    /// Markdown description body (defect details).
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    /// Last mutator's user id (stamped on every mutation).
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_by: Option<String>,
    /// Last mutation time (timestamptz text).
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<String>,
    /// Custom field values, key → string value; field definitions live in the project
    /// template, multi-select values are comma-joined.
    custom_fields: BTreeMap<String, String>,
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
            severity: b.severity,
            handler: b.handler,
            description: b.description,
            updated_by: b.updated_by,
            updated_at: b.updated_at,
            custom_fields: b.custom_fields,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CreateBugRequest {
    project_id: String,
    title: String,
    initial_status: String,
    /// Severity level P0..P3 (optional).
    severity: Option<String>,
    /// Handler user id (optional).
    handler: Option<String>,
    /// Markdown description body (optional).
    description: Option<String>,
    /// Custom field values, key → string value (max 32 keys, key ≤ 64 chars, value ≤ 2000 chars).
    #[serde(default)]
    custom_fields: BTreeMap<String, String>,
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
    match st
        .create
        .execute(
            &req.project_id,
            &req.title,
            &req.initial_status,
            Some(&user.user_id),
            req.severity.as_deref(),
            req.handler.as_deref(),
            req.description.as_deref(),
            &req.custom_fields,
        )
        .await
    {
        Ok(b) => {
            notify_handler(&st, &b, "BUG_ASSIGNED", &b.status, &user.user_id).await;
            (StatusCode::CREATED, Json(BugResponse::from(b))).into_response()
        }
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
    params(("projectId" = String, Query, description = "Project ID")),
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
    match st.change.execute(&id, &req.status, Some(&user.user_id)).await {
        Ok(b) => {
            notify_handler(&st, &b, "BUG_STATUS_CHANGED", &b.status, &user.user_id).await;
            (StatusCode::OK, Json(BugResponse::from(b))).into_response()
        }
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

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct UpdateBugRequest {
    /// New title; absent/blank keeps the current one.
    title: Option<String>,
    /// Severity P0..P3; absent clears.
    severity: Option<String>,
    /// Handler user id; absent clears.
    handler: Option<String>,
    /// Markdown description body; absent keeps the current value.
    description: Option<String>,
}

/// Updates title/severity/handler and stamps updated_by/updated_at with the caller.
#[utoipa::path(put, path = "/bug/{id}", tag = "bug", params(("id" = String, Path)), request_body = UpdateBugRequest, responses((status = 200, body = BugResponse), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_bug(
    user: AuthUser,
    State(st): State<BugState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateBugRequest>,
) -> Response {
    if !user.can("BUG", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st
        .update
        .execute(
            &id,
            req.title.as_deref(),
            req.severity.as_deref(),
            req.handler.as_deref(),
            req.description.as_deref(),
            Some(&user.user_id),
        )
        .await
    {
        Ok(b) => {
            // Only a request that actually (re)assigns a handler notifies them.
            if req.handler.is_some() {
                notify_handler(&st, &b, "BUG_ASSIGNED", &b.status, &user.user_id).await;
            }
            (StatusCode::OK, Json(BugResponse::from(b))).into_response()
        }
        Err(UpdateBugMetaError::BugNotFound) => {
            (StatusCode::NOT_FOUND, "bug not found").into_response()
        }
        Err(UpdateBugMetaError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid bug payload").into_response()
        }
        Err(UpdateBugMetaError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct SetCustomFieldsRequest {
    /// Custom field values, key → string value; full replacement, empty map clears all.
    #[serde(default)]
    custom_fields: BTreeMap<String, String>,
}

/// Replaces the bug's custom field values wholesale; field definitions live in the
/// project template, multi-select values are comma-joined.
#[utoipa::path(put, path = "/bug/{id}/custom-fields", tag = "bug", params(("id" = String, Path)), request_body = SetCustomFieldsRequest, responses((status = 200, body = BugResponse), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn set_custom_fields(
    user: AuthUser,
    State(st): State<BugState>,
    Path(id): Path<String>,
    Json(req): Json<SetCustomFieldsRequest>,
) -> Response {
    if !user.can("BUG", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.custom_fields.replace(&id, &req.custom_fields, Some(&user.user_id)).await {
        Ok(b) => (StatusCode::OK, Json(BugResponse::from(b))).into_response(),
        Err(BugCustomFieldsError::BugNotFound) => {
            (StatusCode::NOT_FOUND, "bug not found").into_response()
        }
        Err(BugCustomFieldsError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid custom fields").into_response()
        }
        Err(BugCustomFieldsError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FollowersResponse {
    followers: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FollowRequest {
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
async fn list_followers(
    user: AuthUser,
    State(st): State<BugState>,
    Path(id): Path<String>,
) -> Response {
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

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RelationItem {
    /// REQUIREMENT | SCENARIO | FUNCTIONAL_CASE | PLAN.
    kind: String,
    target_id: String,
}

impl From<BugRelation> for RelationItem {
    fn from(r: BugRelation) -> Self {
        Self { kind: r.kind.as_str().to_string(), target_id: r.target_id }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RelationsResponse {
    relations: Vec<RelationItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct LinkRelationRequest {
    /// REQUIREMENT | SCENARIO | FUNCTIONAL_CASE | PLAN.
    kind: String,
    target_id: String,
}

fn relations_body(rels: Vec<BugRelation>) -> Json<RelationsResponse> {
    Json(RelationsResponse { relations: rels.into_iter().map(RelationItem::from).collect() })
}

fn map_relation_err(e: BugRelationError) -> Response {
    match e {
        BugRelationError::BugNotFound => (StatusCode::NOT_FOUND, "bug not found").into_response(),
        BugRelationError::Domain(_) => {
            (StatusCode::BAD_REQUEST, "invalid relation payload").into_response()
        }
        BugRelationError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/bug/{id}/relation", tag = "bug", params(("id" = String, Path)), responses((status = 200, body = RelationsResponse), (status = 404)), security(("bearer" = [])))]
async fn list_relations(
    user: AuthUser,
    State(st): State<BugState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("BUG", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.relations.list(&id).await {
        Ok(rels) => (StatusCode::OK, relations_body(rels)).into_response(),
        Err(e) => map_relation_err(e),
    }
}

#[utoipa::path(post, path = "/bug/{id}/relation", tag = "bug", params(("id" = String, Path)), request_body = LinkRelationRequest, responses((status = 200, body = RelationsResponse), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn link_relation(
    user: AuthUser,
    State(st): State<BugState>,
    Path(id): Path<String>,
    Json(req): Json<LinkRelationRequest>,
) -> Response {
    if !user.can("BUG", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.relations.link(&id, &req.kind, &req.target_id).await {
        Ok(rels) => (StatusCode::OK, relations_body(rels)).into_response(),
        Err(e) => map_relation_err(e),
    }
}

#[utoipa::path(delete, path = "/bug/{id}/relation/{kind}/{targetId}", tag = "bug", params(("id" = String, Path), ("kind" = String, Path), ("targetId" = String, Path)), responses((status = 200, body = RelationsResponse), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn unlink_relation(
    user: AuthUser,
    State(st): State<BugState>,
    Path((id, kind, target_id)): Path<(String, String, String)>,
) -> Response {
    if !user.can("BUG", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.relations.unlink(&id, &kind, &target_id).await {
        Ok(rels) => (StatusCode::OK, relations_body(rels)).into_response(),
        Err(e) => map_relation_err(e),
    }
}

/// Reverse lookup: bugs linked to a test plan (kind = PLAN), newest first.
#[utoipa::path(get, path = "/bug/by-plan/{planId}", tag = "bug", params(("planId" = String, Path)), responses((status = 200, body = [BugResponse]), (status = 403)), security(("bearer" = [])))]
async fn list_bugs_by_plan(
    user: AuthUser,
    State(st): State<BugState>,
    Path(plan_id): Path<String>,
) -> Response {
    if !user.can("BUG", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.relations.bugs_for_target("PLAN", &plan_id).await {
        Ok(bugs) => {
            let body: Vec<BugResponse> = bugs.into_iter().map(BugResponse::from).collect();
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_relation_err(e),
    }
}

#[derive(OpenApi)]
#[openapi(paths(create_bug, list_bugs, update_bug, change_status, set_custom_fields, list_followers, follow_bug, unfollow_bug, list_relations, link_relation, unlink_relation, list_bugs_by_plan), components(schemas(CreateBugRequest, UpdateBugRequest, ChangeStatusRequest, SetCustomFieldsRequest, BugResponse, FollowRequest, FollowersResponse, LinkRelationRequest, RelationItem, RelationsResponse)), tags((name = "bug", description = "Defect management")))]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryBugRepository;
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use std::sync::Arc;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app() -> (Router, String) {
        let repo = Arc::new(InMemoryBugRepository::with_default_flow("p1"));
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["BUG:READ+ADD+UPDATE".to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        let r = router(
            CreateBugUseCase::new(repo.clone()),
            ChangeBugStatusUseCase::new(repo.clone()),
            ListBugsUseCase::new(repo.clone()),
            BugFollowersUseCase::new(repo.clone()),
            BugRelationsUseCase::new(repo),
            None,
            sessions,
        );
        (r, token)
    }

    fn post(uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b =
            Request::builder().method("POST").uri(uri).header("content-type", "application/json");
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
            .oneshot(post(
                "/bug",
                r#"{"projectId":"p1","title":"boom","initialStatus":"NEW"}"#,
                Some(t),
            ))
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
        // Status change stamps the caller into the audit pair.
        assert_eq!(v["updatedBy"], "admin");
        assert!(v["updatedAt"].is_string());
    }

    #[tokio::test]
    async fn create_carries_severity_and_handler() {
        let (app, t) = app().await;
        let resp = app
            .clone()
            .oneshot(post(
                "/bug",
                r#"{"projectId":"p1","title":"boom","initialStatus":"NEW","severity":"P1","handler":"bob"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["severity"], "P1");
        assert_eq!(v["handler"], "bob");
        assert_eq!(v["updatedBy"], "admin");
        assert!(v["updatedAt"].is_string());

        // List returns the same fields.
        let resp = app.oneshot(get("/bug?projectId=p1", Some(&t))).await.expect("resp");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v[0]["severity"], "P1");
        assert_eq!(v[0]["handler"], "bob");
        assert_eq!(v[0]["updatedBy"], "admin");
    }

    #[tokio::test]
    async fn create_with_invalid_severity_400() {
        let (app, t) = app().await;
        let resp = app
            .oneshot(post(
                "/bug",
                r#"{"projectId":"p1","title":"boom","initialStatus":"NEW","severity":"HIGH"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn update_bug_meta_roundtrip_and_errors() {
        let (app, t) = app().await;
        let id = create_returns_id(&app, &t).await;

        // Meta update sets fields and stamps the caller.
        let resp = app
            .clone()
            .oneshot(put_req(
                &format!("/bug/{id}"),
                r#"{"title":"boom v2","severity":"P0","handler":"carol"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["title"], "boom v2");
        assert_eq!(v["severity"], "P0");
        assert_eq!(v["handler"], "carol");
        assert_eq!(v["updatedBy"], "admin");
        assert!(v["updatedAt"].is_string());

        // Absent severity/handler clear; absent title keeps.
        let resp = app
            .clone()
            .oneshot(put_req(&format!("/bug/{id}"), r#"{}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["title"], "boom v2");
        assert!(v.get("severity").is_none());
        assert!(v.get("handler").is_none());

        // Invalid severity → 400; missing bug → 404.
        assert_eq!(
            app.clone()
                .oneshot(put_req(&format!("/bug/{id}"), r#"{"severity":"S1"}"#, Some(&t)))
                .await
                .expect("resp")
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            app.oneshot(put_req("/bug/ghost", r#"{}"#, Some(&t))).await.expect("resp").status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn update_bug_without_permission_403() {
        let repo = Arc::new(InMemoryBugRepository::with_default_flow("p1"));
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["BUG:READ+ADD".to_string()]).expect("perms");
        let token = sessions.create("u", perms, 3600).await.expect("token");
        let app = router(
            CreateBugUseCase::new(repo.clone()),
            ChangeBugStatusUseCase::new(repo.clone()),
            ListBugsUseCase::new(repo.clone()),
            BugFollowersUseCase::new(repo.clone()),
            BugRelationsUseCase::new(repo),
            None,
            sessions,
        );
        let id = create_returns_id(&app, &token).await;
        let resp = app
            .oneshot(put_req(&format!("/bug/{id}"), r#"{"severity":"P1"}"#, Some(&token)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    fn put_req(uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b =
            Request::builder().method("PUT").uri(uri).header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    #[tokio::test]
    async fn create_carries_custom_fields_and_put_replaces_them() {
        let (app, t) = app().await;
        // Create with customFields; keys get trimmed.
        let resp = app
            .clone()
            .oneshot(post(
                "/bug",
                r#"{"projectId":"p1","title":"boom","initialStatus":"NEW","customFields":{" severity ":"P0"}}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["customFields"], serde_json::json!({"severity": "P0"}));
        let id = v["id"].as_str().expect("id").to_string();

        // PUT replaces wholesale.
        let resp = app
            .clone()
            .oneshot(put_req(
                &format!("/bug/{id}/custom-fields"),
                r#"{"customFields":{"env":"prod","multi-select":"a,b"}}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["customFields"], serde_json::json!({"env": "prod", "multi-select": "a,b"}));

        // List reads back the replaced fields.
        let resp = app.clone().oneshot(get("/bug?projectId=p1", Some(&t))).await.expect("resp");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v[0]["customFields"], serde_json::json!({"env": "prod", "multi-select": "a,b"}));

        // Blank key → 400; missing bug → 404.
        assert_eq!(
            app.clone()
                .oneshot(put_req(
                    &format!("/bug/{id}/custom-fields"),
                    r#"{"customFields":{"  ":"v"}}"#,
                    Some(&t)
                ))
                .await
                .expect("resp")
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            app.oneshot(put_req("/bug/ghost/custom-fields", r#"{"customFields":{}}"#, Some(&t)))
                .await
                .expect("resp")
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn set_custom_fields_without_update_permission_403() {
        let repo = Arc::new(InMemoryBugRepository::with_default_flow("p1"));
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["BUG:READ+ADD".to_string()]).expect("perms");
        let token = sessions.create("u", perms, 3600).await.expect("token");
        let app = router(
            CreateBugUseCase::new(repo.clone()),
            ChangeBugStatusUseCase::new(repo.clone()),
            ListBugsUseCase::new(repo.clone()),
            BugFollowersUseCase::new(repo.clone()),
            BugRelationsUseCase::new(repo),
            None,
            sessions,
        );
        let id = create_returns_id(&app, &token).await;
        let resp = app
            .oneshot(put_req(
                &format!("/bug/{id}/custom-fields"),
                r#"{"customFields":{"env":"prod"}}"#,
                Some(&token),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
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
        let perms = PermissionSet::from_raw(["BUG:READ+ADD".to_string()]).expect("perms");
        let token = sessions.create("u", perms, 3600).await.expect("token");
        let app = router(
            CreateBugUseCase::new(repo.clone()),
            ChangeBugStatusUseCase::new(repo.clone()),
            ListBugsUseCase::new(repo.clone()),
            BugFollowersUseCase::new(repo.clone()),
            BugRelationsUseCase::new(repo),
            None,
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
            .oneshot(post(
                "/bug",
                r#"{"projectId":"p1","title":"x","initialStatus":"GHOST"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn disallowed_transition_409() {
        let (app, t) = app().await;
        let id = create_returns_id(&app, &t).await;
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
            .oneshot(post(
                "/bug",
                r#"{"projectId":"p1","title":"first","initialStatus":"NEW"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        app.clone()
            .oneshot(post(
                "/bug",
                r#"{"projectId":"p1","title":"second","initialStatus":"NEW"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");

        let resp = app.oneshot(get("/bug?projectId=p1", Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let arr = v.as_array().expect("array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["title"], "second");
        assert_eq!(arr[1]["title"], "first");
    }

    #[tokio::test]
    async fn list_without_permission_403() {
        let repo = Arc::new(InMemoryBugRepository::with_default_flow("p1"));
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["BUG:ADD".to_string()]).expect("perms");
        let token = sessions.create("u", perms, 3600).await.expect("token");
        let app = router(
            CreateBugUseCase::new(repo.clone()),
            ChangeBugStatusUseCase::new(repo.clone()),
            ListBugsUseCase::new(repo.clone()),
            BugFollowersUseCase::new(repo.clone()),
            BugRelationsUseCase::new(repo),
            None,
            sessions,
        );
        let resp = app.oneshot(get("/bug?projectId=p1", Some(&token))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    fn del(uri: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method("DELETE").uri(uri);
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::empty()).expect("req")
    }

    #[tokio::test]
    async fn link_list_unlink_relation_roundtrip() {
        let (app, t) = app().await;
        let id = create_returns_id(&app, &t).await;

        let resp = app
            .clone()
            .oneshot(post(
                &format!("/bug/{id}/relation"),
                r#"{"kind":"REQUIREMENT","targetId":"r1"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = app
            .clone()
            .oneshot(post(
                &format!("/bug/{id}/relation"),
                r#"{"kind":"SCENARIO","targetId":"s1"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["relations"].as_array().expect("arr").len(), 2);

        let resp = app
            .clone()
            .oneshot(del(&format!("/bug/{id}/relation/REQUIREMENT/r1"), Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let rels = v["relations"].as_array().expect("arr");
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0]["kind"], "SCENARIO");
        assert_eq!(rels[0]["targetId"], "s1");
    }

    #[tokio::test]
    async fn plan_relation_reverse_lookup_roundtrip() {
        let (app, t) = app().await;
        let id = create_returns_id(&app, &t).await;

        // Link the bug to a plan, then the reverse route returns it.
        let resp = app
            .clone()
            .oneshot(post(
                &format!("/bug/{id}/relation"),
                r#"{"kind":"PLAN","targetId":"plan-1"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["relations"][0]["kind"], "PLAN");
        assert_eq!(v["relations"][0]["targetId"], "plan-1");

        let resp = app.clone().oneshot(get("/bug/by-plan/plan-1", Some(&t))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let arr = v.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], id.as_str());

        // Other plans stay empty; unlink drops the bug from the reverse view.
        let resp = app.clone().oneshot(get("/bug/by-plan/other", Some(&t))).await.expect("resp");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(v.as_array().expect("array").is_empty());

        let resp = app
            .clone()
            .oneshot(del(&format!("/bug/{id}/relation/PLAN/plan-1"), Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let resp = app.oneshot(get("/bug/by-plan/plan-1", Some(&t))).await.expect("resp");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(v.as_array().expect("array").is_empty());
    }

    #[tokio::test]
    async fn list_bugs_by_plan_without_permission_403() {
        let repo = Arc::new(InMemoryBugRepository::with_default_flow("p1"));
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["BUG:ADD".to_string()]).expect("perms");
        let token = sessions.create("u", perms, 3600).await.expect("token");
        let app = router(
            CreateBugUseCase::new(repo.clone()),
            ChangeBugStatusUseCase::new(repo.clone()),
            ListBugsUseCase::new(repo.clone()),
            BugFollowersUseCase::new(repo.clone()),
            BugRelationsUseCase::new(repo),
            None,
            sessions,
        );
        let resp = app.oneshot(get("/bug/by-plan/plan-1", Some(&token))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn link_relation_invalid_kind_400_and_missing_bug_404() {
        let (app, t) = app().await;
        let id = create_returns_id(&app, &t).await;
        let resp = app
            .clone()
            .oneshot(post(
                &format!("/bug/{id}/relation"),
                r#"{"kind":"king","targetId":"r1"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let resp = app
            .oneshot(post(
                "/bug/ghost/relation",
                r#"{"kind":"REQUIREMENT","targetId":"r1"}"#,
                Some(&t),
            ))
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
