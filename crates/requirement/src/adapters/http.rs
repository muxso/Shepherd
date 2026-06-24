//! 需求管理的 HTTP 适配器。DTO ↔ 用例,领域/命令错误映射 HTTP 码。
//!
//! 写端点经 `webauth::AuthUser` 做 RBAC:创建需 `REQUIREMENT:ADD`,
//! 修订/定基/重命名/归档需 `REQUIREMENT:UPDATE`,删除需 `REQUIREMENT:DELETE`;读端点开放。
//! 错误码:校验→400,标题冲突/归档冲突→409,需求/版本不存在→404。

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
use webauth::{AuthUser, SessionStore};
use utoipa::{IntoParams, OpenApi, ToSchema};

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

// ---- DTO ----

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
    /// 评审不通过原因(必填)。
    comment: String,
}

#[derive(Deserialize, ToSchema)]
struct RenameBody {
    title: String,
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
    /// 最近一次「评审不通过」原因;评审通过或修订后为 null。
    #[serde(skip_serializing_if = "Option::is_none")]
    review_comment: Option<String>,
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

// ---- 错误映射 ----

fn cmd_err(e: RequirementCmdError) -> Response {
    match e {
        RequirementCmdError::Validation(_) => (StatusCode::BAD_REQUEST, "invalid requirement").into_response(),
        RequirementCmdError::TitleExists => (StatusCode::CONFLICT, "title already exists").into_response(),
        RequirementCmdError::NotFound => (StatusCode::NOT_FOUND, "requirement not found").into_response(),
        RequirementCmdError::NoSuchVersion(_) => (StatusCode::NOT_FOUND, "version not found").into_response(),
        RequirementCmdError::Archived => (StatusCode::CONFLICT, "requirement is archived").into_response(),
        RequirementCmdError::NotUnderReview => (StatusCode::CONFLICT, "requirement is not pending review").into_response(),
        RequirementCmdError::Repo(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

fn create_err(e: CreateRequirementError) -> Response {
    match e {
        CreateRequirementError::Validation(_) => (StatusCode::BAD_REQUEST, "invalid requirement").into_response(),
        CreateRequirementError::TitleAlreadyExists => (StatusCode::CONFLICT, "title already exists").into_response(),
        CreateRequirementError::Repo(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

// ---- 处理器 ----

#[utoipa::path(post, path = "/requirement", tag = "requirement", request_body = CreateBody, responses((status = 201, body = RequirementResponse), (status = 401), (status = 403), (status = 409)), security(("bearer" = [])))]
async fn create_requirement(user: AuthUser, State(st): State<ReqState>, Json(b): Json<CreateBody>) -> Response {
    if !user.can("REQUIREMENT", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.create.execute(&b.project_id, &b.title, &b.description, &b.acceptance_criteria).await {
        Ok(r) => (StatusCode::CREATED, Json(RequirementResponse::from(r))).into_response(),
        Err(e) => create_err(e),
    }
}

#[utoipa::path(get, path = "/requirement", tag = "requirement", params(ListQuery), responses((status = 200, body = RequirementPage)))]
async fn list_requirements(State(st): State<ReqState>, Query(q): Query<ListQuery>) -> Response {
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

#[utoipa::path(get, path = "/requirement/{id}", tag = "requirement", params(("id" = String, Path, description = "需求 id")), responses((status = 200, body = RequirementResponse), (status = 404)))]
async fn get_requirement(State(st): State<ReqState>, Path(id): Path<String>) -> Response {
    match st.admin.get(&id).await {
        Ok(r) => (StatusCode::OK, Json(RequirementResponse::from(r))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(get, path = "/requirement/{id}/version/{n}", tag = "requirement", params(("id" = String, Path, description = "需求 id"), ("n" = u32, Path, description = "版本号")), responses((status = 200, body = VersionResponse), (status = 404)))]
async fn get_version(State(st): State<ReqState>, Path((id, n)): Path<(String, u32)>) -> Response {
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

#[utoipa::path(put, path = "/requirement/{id}", tag = "requirement", params(("id" = String, Path)), request_body = RenameBody, responses((status = 200, body = RequirementResponse), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn rename_requirement(
    user: AuthUser,
    State(st): State<ReqState>,
    Path(id): Path<String>,
    Json(b): Json<RenameBody>,
) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.rename(&id, &b.title).await {
        Ok(r) => (StatusCode::OK, Json(RequirementResponse::from(r))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(post, path = "/requirement/{id}/archive", tag = "requirement", params(("id" = String, Path)), responses((status = 200, body = RequirementResponse), (status = 404)), security(("bearer" = [])))]
async fn archive_requirement(user: AuthUser, State(st): State<ReqState>, Path(id): Path<String>) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.archive(&id).await {
        Ok(r) => (StatusCode::OK, Json(RequirementResponse::from(r))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(post, path = "/requirement/{id}/deliver", tag = "requirement", params(("id" = String, Path)), responses((status = 200, body = RequirementResponse), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn deliver_requirement(user: AuthUser, State(st): State<ReqState>, Path(id): Path<String>) -> Response {
    if !user.can("REQUIREMENT", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.deliver(&id).await {
        Ok(r) => (StatusCode::OK, Json(RequirementResponse::from(r))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(delete, path = "/requirement/{id}", tag = "requirement", params(("id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn delete_requirement(user: AuthUser, State(st): State<ReqState>, Path(id): Path<String>) -> Response {
    if !user.can("REQUIREMENT", "DELETE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.delete(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => cmd_err(e),
    }
}

/// 本上下文的 OpenAPI 文档(组装根合并)。
#[derive(OpenApi)]
#[openapi(
    paths(
        create_requirement, list_requirements, get_requirement, get_version,
        revise_requirement, set_baseline, reject_review, rename_requirement, archive_requirement, deliver_requirement, delete_requirement
    ),
    components(schemas(CreateBody, ReviseBody, SetBaselineBody, RejectReviewBody, RenameBody, VersionResponse, RequirementResponse, RequirementPage, VersionCreated)),
    tags((name = "requirement", description = "需求管理(多版本)"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use crate::adapters::InMemoryRequirementRepository;
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
        let mut b = Request::builder().method(method).uri(uri).header("content-type", "application/json");
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
    async fn full_versioning_crud_flow() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD+UPDATE+DELETE").await;
        // create
        let r = app
            .clone()
            .oneshot(req("POST", "/requirement", r#"{"projectId":"p1","title":"登录","description":"d","acceptanceCriteria":["c1"]}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let id = body_json(r).await["id"].as_str().expect("id").to_string();

        // list / get
        assert_eq!(
            app.clone().oneshot(req("GET", "/requirement?projectId=p1&current=1&pageSize=10", "", Some(&t))).await.expect("r").status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone().oneshot(req("GET", &format!("/requirement/{id}"), "", Some(&t))).await.expect("r").status(),
            StatusCode::OK
        );

        // revise → v2
        let rv = app
            .clone()
            .oneshot(req("POST", &format!("/requirement/{id}/version"), r#"{"description":"v2","acceptanceCriteria":["c2"]}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(rv.status(), StatusCode::CREATED);
        assert_eq!(body_json(rv).await["version"], 2);

        // get version 2
        assert_eq!(
            app.clone().oneshot(req("GET", &format!("/requirement/{id}/version/2"), "", Some(&t))).await.expect("r").status(),
            StatusCode::OK
        );
        // unknown version → 404
        assert_eq!(
            app.clone().oneshot(req("GET", &format!("/requirement/{id}/version/9"), "", Some(&t))).await.expect("r").status(),
            StatusCode::NOT_FOUND
        );

        // set baseline → 2
        let sb = app
            .clone()
            .oneshot(req("PUT", &format!("/requirement/{id}/baseline"), r#"{"version":2}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(sb.status(), StatusCode::OK);
        let v = body_json(sb).await;
        assert_eq!(v["baselineVersion"], 2);
        assert_eq!(v["status"], "BASELINED");

        // rename
        assert_eq!(
            app.clone().oneshot(req("PUT", &format!("/requirement/{id}"), r#"{"title":"登入"}"#, Some(&t))).await.expect("r").status(),
            StatusCode::OK
        );
        // archive then revise → 409
        assert_eq!(
            app.clone().oneshot(req("POST", &format!("/requirement/{id}/archive"), "", Some(&t))).await.expect("r").status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone().oneshot(req("POST", &format!("/requirement/{id}/version"), r#"{"description":"v3"}"#, Some(&t))).await.expect("r").status(),
            StatusCode::CONFLICT
        );
        // delete → 204 then get 404
        assert_eq!(
            app.clone().oneshot(req("DELETE", &format!("/requirement/{id}"), "", Some(&t))).await.expect("r").status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            app.oneshot(req("GET", &format!("/requirement/{id}"), "", Some(&t))).await.expect("r").status(),
            StatusCode::NOT_FOUND
        );
    }

    /// 详情必须把验收标准放在「基线版本」那条 `versions[]` 里(camelCase),
    /// 这正是前端 `versions.find(v => v.version === baselineVersion).acceptanceCriteria` 读取的契约。
    /// 覆盖两点:新建即可见(基线=v1);改基线后详情随基线版本切换。
    #[tokio::test]
    async fn detail_exposes_acceptance_criteria_at_baseline_version() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD+UPDATE").await;
        // 新建带验收标准
        let r = app
            .clone()
            .oneshot(req("POST", "/requirement", r#"{"projectId":"p1","title":"登录","acceptanceCriteria":["登录成功跳转首页","错误密码拒绝并提示"]}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let id = body_json(r).await["id"].as_str().expect("id").to_string();

        // 模拟前端取基线版本的验收标准
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

        // 详情:基线 = v1,验收标准随之可见
        let d = body_json(app.clone().oneshot(req("GET", &format!("/requirement/{id}"), "", Some(&t))).await.expect("r")).await;
        assert_eq!(d["baselineVersion"], 1);
        assert_eq!(crits_at_baseline(&d), vec!["登录成功跳转首页".to_string(), "错误密码拒绝并提示".to_string()]);

        // 追加 v2(不同标准)并改基线 → 详情按基线版本切换
        app.clone()
            .oneshot(req("POST", &format!("/requirement/{id}/version"), r#"{"description":"v2","acceptanceCriteria":["新增验收点"]}"#, Some(&t)))
            .await
            .expect("r");
        app.clone()
            .oneshot(req("PUT", &format!("/requirement/{id}/baseline"), r#"{"version":2}"#, Some(&t)))
            .await
            .expect("r");
        let d2 = body_json(app.oneshot(req("GET", &format!("/requirement/{id}"), "", Some(&t))).await.expect("r")).await;
        assert_eq!(d2["baselineVersion"], 2);
        assert_eq!(crits_at_baseline(&d2), vec!["新增验收点".to_string()]);
    }

    /// 评审不通过:原因必填(空 → 400);带原因 → 200 且 reviewComment 暴露给前端;
    /// 评审通过(定基线)后 reviewComment 清空且不再出现在响应里。
    #[tokio::test]
    async fn reject_review_flow() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD+UPDATE").await;
        let r = app
            .clone()
            .oneshot(req("POST", "/requirement", r#"{"projectId":"p1","title":"登录","acceptanceCriteria":["c1"]}"#, Some(&t)))
            .await
            .expect("r");
        let id = body_json(r).await["id"].as_str().expect("id").to_string();

        // 空原因 → 400
        assert_eq!(
            app.clone().oneshot(req("POST", &format!("/requirement/{id}/review/reject"), r#"{"comment":"   "}"#, Some(&t))).await.expect("r").status(),
            StatusCode::BAD_REQUEST
        );

        // 带原因 → 200,状态仍 DRAFT,reviewComment 可见
        let rj = app
            .clone()
            .oneshot(req("POST", &format!("/requirement/{id}/review/reject"), r#"{"comment":"验收标准不完整"}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(rj.status(), StatusCode::OK);
        let v = body_json(rj).await;
        assert_eq!(v["status"], "DRAFT");
        assert_eq!(v["reviewComment"], "验收标准不完整");

        // 评审通过(定基线 v1)→ reviewComment 清空(字段被省略)
        let sb = app
            .clone()
            .oneshot(req("PUT", &format!("/requirement/{id}/baseline"), r#"{"version":1}"#, Some(&t)))
            .await
            .expect("r");
        let v2 = body_json(sb).await;
        assert_eq!(v2["status"], "BASELINED");
        assert!(v2.get("reviewComment").is_none());

        // 已基线再评不通过 → 409
        assert_eq!(
            app.oneshot(req("POST", &format!("/requirement/{id}/review/reject"), r#"{"comment":"x"}"#, Some(&t))).await.expect("r").status(),
            StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn create_requires_token_and_permission() {
        let (app, _t) = app_with("REQUIREMENT:READ").await; // 只读
        // 无令牌 → 401
        assert_eq!(
            app.clone().oneshot(req("POST", "/requirement", r#"{"projectId":"p1","title":"x"}"#, None)).await.expect("r").status(),
            StatusCode::UNAUTHORIZED
        );
        // 有令牌但无 ADD → 403
        let (app2, t2) = app_with("REQUIREMENT:READ").await;
        assert_eq!(
            app2.oneshot(req("POST", "/requirement", r#"{"projectId":"p1","title":"x"}"#, Some(&t2))).await.expect("r").status(),
            StatusCode::FORBIDDEN
        );
        let _ = app;
    }

    #[tokio::test]
    async fn duplicate_title_409() {
        let (app, t) = app_with("REQUIREMENT:READ+ADD").await;
        let body = r#"{"projectId":"p1","title":"唯一"}"#;
        assert_eq!(
            app.clone().oneshot(req("POST", "/requirement", body, Some(&t))).await.expect("r").status(),
            StatusCode::CREATED
        );
        assert_eq!(
            app.oneshot(req("POST", "/requirement", body, Some(&t))).await.expect("r").status(),
            StatusCode::CONFLICT
        );
    }
}
