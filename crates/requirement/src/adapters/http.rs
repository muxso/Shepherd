use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use kernel::page::PageRequest;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use crate::application::{
    CreateRequirementError, CreateRequirementUseCase, ListRequirementsUseCase, RequirementCmdError,
    RequirementService,
};
use crate::domain::Requirement;

#[derive(Clone)]
struct ReqState {
    create: CreateRequirementUseCase,
    list: ListRequirementsUseCase,
    admin: RequirementService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<ReqState> for Arc<dyn SessionStore> {
    fn from_ref(s: &ReqState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    create: CreateRequirementUseCase,
    list: ListRequirementsUseCase,
    admin: RequirementService,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/requirement", post(create_requirement).get(list_requirements))
        .route("/requirement/order", put(reorder_requirements))
        .route("/requirement/summary", get(requirement_summary))
        .route(
            "/requirement/{id}",
            get(get_requirement).put(rename_requirement).delete(delete_requirement),
        )
        .route("/requirement/{id}/version", post(revise_requirement))
        .route("/requirement/{id}/version/{n}", get(get_version))
        .route("/requirement/{id}/baseline", put(set_baseline))
        .route("/requirement/{id}/review/reject", post(reject_review))
        .route("/requirement/{id}/archive", post(archive_requirement))
        .route("/requirement/{id}/deliver", post(deliver_requirement))
        .with_state(ReqState { create, list, admin, sessions })
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = RequirementCreateBody)]
struct CreateBody {
    project_id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
    /// 优先级 P0/P1/P2/P3(不区分大小写),缺省 P2。
    #[serde(default)]
    priority: Option<String>,
    /// 需求类型 FEATURE/ENHANCEMENT/TECH_DEBT/BUGFIX(不区分大小写),缺省 FEATURE。
    #[serde(default)]
    req_type: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReviseBody {
    #[serde(default)]
    description: String,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
struct SetBaselineBody {
    version: u32,
}

#[derive(Deserialize, ToSchema)]
struct RejectReviewBody {
    comment: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RenameBody {
    title: String,
    /// 可选:更新优先级,缺省不动。
    #[serde(default)]
    priority: Option<String>,
    /// 可选:更新需求类型,缺省不动。
    #[serde(default)]
    req_type: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct VersionResponse {
    version: u32,
    description: String,
    acceptance_criteria: Vec<String>,
}

impl From<&crate::domain::RequirementVersion> for VersionResponse {
    fn from(v: &crate::domain::RequirementVersion) -> Self {
        Self {
            version: v.version,
            description: v.description.clone(),
            acceptance_criteria: v.acceptance_criteria.iter().map(|c| c.text.clone()).collect(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RequirementResponse {
    id: String,
    project_id: String,
    title: String,
    status: String,
    baseline_version: u32,
    latest_version: u32,
    versions: Vec<VersionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    review_comment: Option<String>,
    priority: String,
    req_type: String,
}

impl From<Requirement> for RequirementResponse {
    fn from(r: Requirement) -> Self {
        let latest_version = r.latest_version();
        let versions = r.versions.iter().map(VersionResponse::from).collect();
        Self {
            id: r.id,
            project_id: r.project_id,
            title: r.title,
            status: r.status.as_str().to_string(),
            baseline_version: r.baseline_version,
            latest_version,
            versions,
            review_comment: r.review_comment,
            priority: r.priority.as_str().to_string(),
            req_type: r.req_type.as_str().to_string(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RequirementPage {
    total: u64,
    current: u32,
    page_size: u32,
    total_pages: u64,
    items: Vec<RequirementResponse>,
}

#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    project_id: String,
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

fn cmd_err(e: RequirementCmdError) -> Response {
    match e {
        RequirementCmdError::Validation(_) => {
            (StatusCode::BAD_REQUEST, "invalid requirement").into_response()
        }
        RequirementCmdError::TitleExists => {
            (StatusCode::CONFLICT, "title already exists").into_response()
        }
        RequirementCmdError::NotFound => {
            (StatusCode::NOT_FOUND, "requirement not found").into_response()
        }
        RequirementCmdError::NoSuchVersion(_) => {
            (StatusCode::NOT_FOUND, "version not found").into_response()
        }
        RequirementCmdError::Archived => {
            (StatusCode::CONFLICT, "requirement is archived").into_response()
        }
        RequirementCmdError::NotUnderReview => {
            (StatusCode::CONFLICT, "requirement is not pending review").into_response()
        }
        RequirementCmdError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

fn create_err(e: CreateRequirementError) -> Response {
    match e {
        CreateRequirementError::Validation(_) => {
            (StatusCode::BAD_REQUEST, "invalid requirement").into_response()
        }
        CreateRequirementError::TitleAlreadyExists => {
            (StatusCode::CONFLICT, "title already exists").into_response()
        }
        CreateRequirementError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(post, path = "/requirement", tag = "requirement", request_body = CreateBody, responses((status = 201, body = RequirementResponse), (status = 400), (status = 401), (status = 403), (status = 409)), security(("bearer" = [])))]
async fn create_requirement(
    user: AuthUser,
    State(st): State<ReqState>,
    Json(b): Json<CreateBody>,
) -> Response {
    if !user.can("REQUIREMENT", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st
        .create
        .execute_with(
            &b.project_id,
            &b.title,
            &b.description,
            &b.acceptance_criteria,
            b.priority.as_deref(),
            b.req_type.as_deref(),
        )
        .await
    {
        Ok(r) => (StatusCode::CREATED, Json(RequirementResponse::from(r))).into_response(),
        Err(e) => create_err(e),
    }
}

#[utoipa::path(get, path = "/requirement", tag = "requirement", params(ListQuery), responses((status = 200, body = RequirementPage), (status = 403)), security(("bearer" = [])))]
async fn list_requirements(
    user: AuthUser,
    State(st): State<ReqState>,
    Query(q): Query<ListQuery>,
) -> Response {
    if !user.can("REQUIREMENT", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let page = match PageRequest::new(q.current, q.page_size) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid page params").into_response(),
    };
    match st.list.execute(&q.project_id, page).await {
        Ok(p) => {
            let body = RequirementPage {
                total: p.total,
                current: p.current,
                page_size: p.page_size,
                total_pages: p.total_pages(),
                items: p.items.into_iter().map(RequirementResponse::from).collect(),
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(get, path = "/requirement/{id}", tag = "requirement", params(("id" = String, Path, description = "需求 id")), responses((status = 200, body = RequirementResponse), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn get_requirement(
    user: AuthUser,
    State(st): State<ReqState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("REQUIREMENT", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.get(&id).await {
        Ok(r) => (StatusCode::OK, Json(RequirementResponse::from(r))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(get, path = "/requirement/{id}/version/{n}", tag = "requirement", params(("id" = String, Path, description = "需求 id"), ("n" = u32, Path, description = "版本号")), responses((status = 200, body = VersionResponse), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn get_version(
    user: AuthUser,
    State(st): State<ReqState>,
    Path((id, n)): Path<(String, u32)>,
) -> Response {
    if !user.can("REQUIREMENT", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.get(&id).await {
        Ok(r) => match r.version(n) {
            Some(v) => (StatusCode::OK, Json(VersionResponse::from(v))).into_response(),
            None => (StatusCode::NOT_FOUND, "version not found").into_response(),
        },
        Err(e) => cmd_err(e),
    }
}

#[derive(Serialize, ToSchema)]
struct VersionCreated {
    version: u32,
}

#[utoipa::path(post, path = "/requirement/{id}/version", tag = "requirement", params(("id" = String, Path)), request_body = ReviseBody, responses((status = 201, body = VersionCreated), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn revise_requirement(
    user: AuthUser,
    State(st): State<ReqState>,
    Path(id): Path<String>,
    Json(b): Json<ReviseBody>,
) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.revise(&id, &b.description, &b.acceptance_criteria).await {
        Ok(version) => (StatusCode::CREATED, Json(VersionCreated { version })).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(put, path = "/requirement/{id}/baseline", tag = "requirement", params(("id" = String, Path)), request_body = SetBaselineBody, responses((status = 200, body = RequirementResponse), (status = 404)), security(("bearer" = [])))]
async fn set_baseline(
    user: AuthUser,
    State(st): State<ReqState>,
    Path(id): Path<String>,
    Json(b): Json<SetBaselineBody>,
) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.set_baseline(&id, b.version).await {
        Ok(r) => (StatusCode::OK, Json(RequirementResponse::from(r))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(post, path = "/requirement/{id}/review/reject", tag = "requirement", params(("id" = String, Path)), request_body = RejectReviewBody, responses((status = 200, body = RequirementResponse), (status = 400), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn reject_review(
    user: AuthUser,
    State(st): State<ReqState>,
    Path(id): Path<String>,
    Json(b): Json<RejectReviewBody>,
) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.reject_review(&id, &b.comment).await {
        Ok(r) => (StatusCode::OK, Json(RequirementResponse::from(r))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(put, path = "/requirement/{id}", tag = "requirement", params(("id" = String, Path)), request_body = RenameBody, responses((status = 200, body = RequirementResponse), (status = 400), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn rename_requirement(
    user: AuthUser,
    State(st): State<ReqState>,
    Path(id): Path<String>,
    Json(b): Json<RenameBody>,
) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.update(&id, &b.title, b.priority.as_deref(), b.req_type.as_deref()).await {
        Ok(r) => (StatusCode::OK, Json(RequirementResponse::from(r))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(post, path = "/requirement/{id}/archive", tag = "requirement", params(("id" = String, Path)), responses((status = 200, body = RequirementResponse), (status = 404)), security(("bearer" = [])))]
async fn archive_requirement(
    user: AuthUser,
    State(st): State<ReqState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.archive(&id).await {
        Ok(r) => (StatusCode::OK, Json(RequirementResponse::from(r))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(post, path = "/requirement/{id}/deliver", tag = "requirement", params(("id" = String, Path)), responses((status = 200, body = RequirementResponse), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn deliver_requirement(
    user: AuthUser,
    State(st): State<ReqState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.deliver(&id).await {
        Ok(r) => (StatusCode::OK, Json(RequirementResponse::from(r))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(delete, path = "/requirement/{id}", tag = "requirement", params(("id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn delete_requirement(
    user: AuthUser,
    State(st): State<ReqState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("REQUIREMENT", "DELETE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.delete(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => cmd_err(e),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReorderBody {
    project_id: String,
    ordered_ids: Vec<String>,
}

/// 手工排序:按 `orderedIds` 顺序为项目内这些需求写入显式秩;列表按该秩展示。
#[utoipa::path(put, path = "/requirement/order", tag = "requirement", request_body = ReorderBody, responses((status = 204), (status = 403)), security(("bearer" = [])))]
async fn reorder_requirements(
    user: AuthUser,
    State(st): State<ReqState>,
    Json(b): Json<ReorderBody>,
) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.reorder(&b.project_id, &b.ordered_ids).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => cmd_err(e),
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct SummaryQuery {
    project_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct SummaryResponse {
    total: u64,
    draft: u64,
    baselined: u64,
    delivered: u64,
    archived: u64,
}

/// 项目需求按状态聚合(仪表盘):总数 + 各状态计数。
#[utoipa::path(get, path = "/requirement/summary", tag = "requirement", params(SummaryQuery), responses((status = 200, body = SummaryResponse), (status = 403)), security(("bearer" = [])))]
async fn requirement_summary(
    user: AuthUser,
    State(st): State<ReqState>,
    Query(q): Query<SummaryQuery>,
) -> Response {
    if !user.can("REQUIREMENT", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.status_summary(&q.project_id).await {
        Ok(c) => (
            StatusCode::OK,
            Json(SummaryResponse {
                total: c.total(),
                draft: c.draft,
                baselined: c.baselined,
                delivered: c.delivered,
                archived: c.archived,
            }),
        )
            .into_response(),
        Err(e) => cmd_err(e),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        create_requirement, list_requirements, get_requirement, get_version,
        revise_requirement, set_baseline, reject_review, rename_requirement, archive_requirement, deliver_requirement, delete_requirement, reorder_requirements, requirement_summary
    ),
    components(schemas(CreateBody, ReviseBody, SetBaselineBody, RejectReviewBody, RenameBody, ReorderBody, SummaryResponse, VersionResponse, RequirementResponse, RequirementPage, VersionCreated)),
    tags((name = "requirement", description = "需求管理(多版本)"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryRequirementRepository;
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app_with(perms: &str) -> (Router, String) {
        let repo = Arc::new(InMemoryRequirementRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let set = PermissionSet::from_raw([perms.to_string()]).expect("perms");
        let token = sessions.create("u", set, 3600).await.expect("token");
        let r = router(
            CreateRequirementUseCase::new(repo.clone()),
            ListRequirementsUseCase::new(repo.clone()),
            RequirementService::new(repo),
            sessions,
        );
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

    async fn create_req(app: &Router, t: &str, title: &str) -> String {
        let r = app
            .clone()
            .oneshot(req(
                "POST",
                "/requirement",
                &format!(r#"{{"projectId":"p1","title":"{title}","description":"d","acceptanceCriteria":[]}}"#),
                Some(t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        body_json(r).await["id"].as_str().expect("id").to_string()
    }

    #[tokio::test]
    async fn reorder_changes_list_order_not_caught_by_id_route() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD+UPDATE").await;
        let a = create_req(&app, &t, "A").await;
        let b = create_req(&app, &t, "B").await;

        // PUT /requirement/order 命中 reorder(204),而非 /requirement/{id} 改名。
        let body = format!(r#"{{"projectId":"p1","orderedIds":["{b}","{a}"]}}"#);
        let r = app
            .clone()
            .oneshot(req("PUT", "/requirement/order", &body, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::NO_CONTENT);

        let r = app
            .oneshot(req("GET", "/requirement?projectId=p1&current=1&pageSize=10", "", Some(&t)))
            .await
            .expect("r");
        let v = body_json(r).await;
        assert_eq!(v["items"][0]["title"], "B");
        assert_eq!(v["items"][1]["title"], "A");
    }

    #[tokio::test]
    async fn summary_aggregates_by_status() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD+UPDATE").await;
        create_req(&app, &t, "A").await;
        let b = create_req(&app, &t, "B").await;
        // B → baselined。
        let r = app
            .clone()
            .oneshot(req(
                "PUT",
                &format!("/requirement/{b}/baseline"),
                r#"{"version":1}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::OK);

        let r = app
            .oneshot(req("GET", "/requirement/summary?projectId=p1", "", Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::OK);
        let v = body_json(r).await;
        assert_eq!(v["total"], 2);
        assert_eq!(v["draft"], 1);
        assert_eq!(v["baselined"], 1);
    }

    #[tokio::test]
    async fn summary_without_read_perm_403() {
        // 无任何权限的会话。
        let repo = Arc::new(InMemoryRequirementRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let set = PermissionSet::from_raw(["BUG:READ".to_string()]).expect("perms");
        let token = sessions.create("u", set, 3600).await.expect("token");
        let app = router(
            CreateRequirementUseCase::new(repo.clone()),
            ListRequirementsUseCase::new(repo.clone()),
            RequirementService::new(repo),
            sessions,
        );
        let r = app
            .oneshot(req("GET", "/requirement/summary?projectId=p1", "", Some(&token)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn reorder_without_update_perm_403() {
        let (app, t) = app_with("REQUIREMENT:READ").await;
        let r = app
            .oneshot(req(
                "PUT",
                "/requirement/order",
                r#"{"projectId":"p1","orderedIds":[]}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn full_versioning_crud_flow() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD+UPDATE+DELETE").await;
        let r = app
            .clone()
            .oneshot(req("POST", "/requirement", r#"{"projectId":"p1","title":"登录","description":"d","acceptanceCriteria":["c1"]}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let id = body_json(r).await["id"].as_str().expect("id").to_string();

        assert_eq!(
            app.clone()
                .oneshot(req(
                    "GET",
                    "/requirement?projectId=p1&current=1&pageSize=10",
                    "",
                    Some(&t)
                ))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req("GET", &format!("/requirement/{id}"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );

        let rv = app
            .clone()
            .oneshot(req(
                "POST",
                &format!("/requirement/{id}/version"),
                r#"{"description":"v2","acceptanceCriteria":["c2"]}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(rv.status(), StatusCode::CREATED);
        assert_eq!(body_json(rv).await["version"], 2);

        assert_eq!(
            app.clone()
                .oneshot(req("GET", &format!("/requirement/{id}/version/2"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req("GET", &format!("/requirement/{id}/version/9"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::NOT_FOUND
        );

        let sb = app
            .clone()
            .oneshot(req(
                "PUT",
                &format!("/requirement/{id}/baseline"),
                r#"{"version":2}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(sb.status(), StatusCode::OK);
        let v = body_json(sb).await;
        assert_eq!(v["baselineVersion"], 2);
        assert_eq!(v["status"], "BASELINED");

        assert_eq!(
            app.clone()
                .oneshot(req("PUT", &format!("/requirement/{id}"), r#"{"title":"登入"}"#, Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req("POST", &format!("/requirement/{id}/archive"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req(
                    "POST",
                    &format!("/requirement/{id}/version"),
                    r#"{"description":"v3"}"#,
                    Some(&t)
                ))
                .await
                .expect("r")
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            app.clone()
                .oneshot(req("DELETE", &format!("/requirement/{id}"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            app.oneshot(req("GET", &format!("/requirement/{id}"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    /// 验收标准必须放在「基线版本」那条 `versions[]` 里(前端按 baselineVersion 检索)。
    #[tokio::test]
    async fn detail_exposes_acceptance_criteria_at_baseline_version() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD+UPDATE").await;
        let r = app
            .clone()
            .oneshot(req("POST", "/requirement", r#"{"projectId":"p1","title":"登录","acceptanceCriteria":["登录成功跳转首页","错误密码拒绝并提示"]}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let id = body_json(r).await["id"].as_str().expect("id").to_string();

        let crits_at_baseline = |v: &serde_json::Value| -> Vec<String> {
            let base = v["baselineVersion"].as_u64().expect("baselineVersion");
            v["versions"]
                .as_array()
                .expect("versions")
                .iter()
                .find(|ver| ver["version"].as_u64() == Some(base))
                .expect("baseline version present")["acceptanceCriteria"]
                .as_array()
                .expect("acceptanceCriteria")
                .iter()
                .map(|c| c.as_str().expect("str").to_string())
                .collect()
        };

        let d = body_json(
            app.clone()
                .oneshot(req("GET", &format!("/requirement/{id}"), "", Some(&t)))
                .await
                .expect("r"),
        )
        .await;
        assert_eq!(d["baselineVersion"], 1);
        assert_eq!(
            crits_at_baseline(&d),
            vec!["登录成功跳转首页".to_string(), "错误密码拒绝并提示".to_string()]
        );

        app.clone()
            .oneshot(req(
                "POST",
                &format!("/requirement/{id}/version"),
                r#"{"description":"v2","acceptanceCriteria":["新增验收点"]}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        app.clone()
            .oneshot(req(
                "PUT",
                &format!("/requirement/{id}/baseline"),
                r#"{"version":2}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        let d2 = body_json(
            app.oneshot(req("GET", &format!("/requirement/{id}"), "", Some(&t))).await.expect("r"),
        )
        .await;
        assert_eq!(d2["baselineVersion"], 2);
        assert_eq!(crits_at_baseline(&d2), vec!["新增验收点".to_string()]);
    }

    #[tokio::test]
    async fn reject_review_flow() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD+UPDATE").await;
        let r = app
            .clone()
            .oneshot(req(
                "POST",
                "/requirement",
                r#"{"projectId":"p1","title":"登录","acceptanceCriteria":["c1"]}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        let id = body_json(r).await["id"].as_str().expect("id").to_string();

        assert_eq!(
            app.clone()
                .oneshot(req(
                    "POST",
                    &format!("/requirement/{id}/review/reject"),
                    r#"{"comment":"   "}"#,
                    Some(&t)
                ))
                .await
                .expect("r")
                .status(),
            StatusCode::BAD_REQUEST
        );

        let rj = app
            .clone()
            .oneshot(req(
                "POST",
                &format!("/requirement/{id}/review/reject"),
                r#"{"comment":"验收标准不完整"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(rj.status(), StatusCode::OK);
        let v = body_json(rj).await;
        assert_eq!(v["status"], "DRAFT");
        assert_eq!(v["reviewComment"], "验收标准不完整");

        let sb = app
            .clone()
            .oneshot(req(
                "PUT",
                &format!("/requirement/{id}/baseline"),
                r#"{"version":1}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        let v2 = body_json(sb).await;
        assert_eq!(v2["status"], "BASELINED");
        assert!(v2.get("reviewComment").is_none());

        assert_eq!(
            app.oneshot(req(
                "POST",
                &format!("/requirement/{id}/review/reject"),
                r#"{"comment":"x"}"#,
                Some(&t)
            ))
            .await
            .expect("r")
            .status(),
            StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn create_defaults_priority_and_req_type() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD").await;
        let r = app
            .clone()
            .oneshot(req("POST", "/requirement", r#"{"projectId":"p1","title":"登录"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = body_json(r).await;
        assert_eq!(v["priority"], "P2");
        assert_eq!(v["reqType"], "FEATURE");

        let id = v["id"].as_str().expect("id");
        let d = body_json(
            app.oneshot(req("GET", &format!("/requirement/{id}"), "", Some(&t))).await.expect("r"),
        )
        .await;
        assert_eq!(d["priority"], "P2");
        assert_eq!(d["reqType"], "FEATURE");
    }

    #[tokio::test]
    async fn create_accepts_explicit_priority_and_req_type_normalized() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD").await;
        let r = app
            .clone()
            .oneshot(req(
                "POST",
                "/requirement",
                r#"{"projectId":"p1","title":"登录","priority":" p0 ","reqType":"tech_debt"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = body_json(r).await;
        assert_eq!(v["priority"], "P0");
        assert_eq!(v["reqType"], "TECH_DEBT");
    }

    #[tokio::test]
    async fn create_invalid_priority_or_req_type_400() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD").await;
        assert_eq!(
            app.clone()
                .oneshot(req(
                    "POST",
                    "/requirement",
                    r#"{"projectId":"p1","title":"登录","priority":"P9"}"#,
                    Some(&t)
                ))
                .await
                .expect("r")
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            app.oneshot(req(
                "POST",
                "/requirement",
                r#"{"projectId":"p1","title":"登录","reqType":"EPIC"}"#,
                Some(&t)
            ))
            .await
            .expect("r")
            .status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn put_updates_priority_and_req_type_and_omission_keeps_them() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD+UPDATE").await;
        let id = create_req(&app, &t, "登录").await;

        let r = app
            .clone()
            .oneshot(req(
                "PUT",
                &format!("/requirement/{id}"),
                r#"{"title":"登录","priority":"p1","reqType":"bugfix"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::OK);
        let v = body_json(r).await;
        assert_eq!(v["priority"], "P1");
        assert_eq!(v["reqType"], "BUGFIX");

        // 只改名不带优先级/类型 → 保留原值。
        let r2 = app
            .clone()
            .oneshot(req("PUT", &format!("/requirement/{id}"), r#"{"title":"登入"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r2.status(), StatusCode::OK);
        let v2 = body_json(r2).await;
        assert_eq!(v2["title"], "登入");
        assert_eq!(v2["priority"], "P1");
        assert_eq!(v2["reqType"], "BUGFIX");

        // 非法优先级 → 400。
        assert_eq!(
            app.oneshot(req(
                "PUT",
                &format!("/requirement/{id}"),
                r#"{"title":"登入","priority":"高"}"#,
                Some(&t)
            ))
            .await
            .expect("r")
            .status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn create_requires_token_and_permission() {
        let (app, _t) = app_with("REQUIREMENT:READ").await;
        assert_eq!(
            app.clone()
                .oneshot(req("POST", "/requirement", r#"{"projectId":"p1","title":"x"}"#, None))
                .await
                .expect("r")
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let (app2, t2) = app_with("REQUIREMENT:READ").await;
        assert_eq!(
            app2.oneshot(req(
                "POST",
                "/requirement",
                r#"{"projectId":"p1","title":"x"}"#,
                Some(&t2)
            ))
            .await
            .expect("r")
            .status(),
            StatusCode::FORBIDDEN
        );
        let _ = app;
    }

    #[tokio::test]
    async fn duplicate_title_409() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD").await;
        let body = r#"{"projectId":"p1","title":"唯一"}"#;
        assert_eq!(
            app.clone()
                .oneshot(req("POST", "/requirement", body, Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::CREATED
        );
        assert_eq!(
            app.oneshot(req("POST", "/requirement", body, Some(&t))).await.expect("r").status(),
            StatusCode::CONFLICT
        );
    }
}
