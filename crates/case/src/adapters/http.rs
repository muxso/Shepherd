use std::sync::Arc;

use crate::application::{
    DeleteReviewError, DeleteReviewUseCase, SubmitReviewError, SubmitReviewUseCase,
    UpdateReviewMetaError, UpdateReviewMetaUseCase,
};
use crate::domain::{ReviewError, ReviewStatus, Verdict};
use crate::ports::{NewReview, RepoError, ReviewMeta, ReviewMetaUpdate, ReviewRepository};
use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use notice::application::{NoticeEvent, Notifier};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct ReviewState {
    use_case: SubmitReviewUseCase,
    update_meta: UpdateReviewMetaUseCase,
    delete_review: DeleteReviewUseCase,
    repo: Arc<dyn ReviewRepository>,
    notifier: Option<Notifier>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<ReviewState> for Arc<dyn SessionStore> {
    fn from_ref(s: &ReviewState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    use_case: SubmitReviewUseCase,
    repo: Arc<dyn ReviewRepository>,
    notifier: Option<Notifier>,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    let update_meta = UpdateReviewMetaUseCase::new(repo.clone());
    let delete_review = DeleteReviewUseCase::new(repo.clone());
    Router::new()
        .route("/case-review", post(create_review).get(list_reviews))
        .route(
            "/case-review/{review_id}",
            get(get_review).put(update_review).delete(delete_review_handler),
        )
        .route("/case-review/{review_id}/{case_id}", post(submit_review))
        .with_state(ReviewState { use_case, update_meta, delete_review, repo, notifier, sessions })
}

fn repo_err(e: RepoError) -> Response {
    match e {
        RepoError::NotFound => (StatusCode::NOT_FOUND, "review not found").into_response(),
        RepoError::Backend(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CreateReviewRequest {
    project_id: String,
    #[serde(default)]
    pass_rule: String,
    #[serde(default = "one")]
    reviewer_count: usize,
    #[serde(default)]
    case_ids: Vec<String>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    module_id: Option<String>,
    #[serde(default)]
    start_at: Option<String>,
    #[serde(default)]
    end_at: Option<String>,
}
fn one() -> usize {
    1
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct UpdateReviewRequest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    module_id: Option<String>,
    #[serde(default)]
    start_at: Option<String>,
    #[serde(default)]
    end_at: Option<String>,
    #[serde(default)]
    pass_rule: String,
    #[serde(default = "one")]
    reviewer_count: usize,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CreatedReview {
    id: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReviewSummaryResponse {
    id: String,
    pass_rule: String,
    reviewer_count: usize,
    total: usize,
    passed: usize,
    created_at: String,
    status: String,
    name: String,
    description: String,
    tags: Vec<String>,
    module_id: Option<String>,
    start_at: Option<String>,
    end_at: Option<String>,
    created_by: Option<String>,
    reviewers: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReviewCaseStatusResponse {
    case_id: String,
    status: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReviewDetailResponse {
    id: String,
    pass_rule: String,
    reviewer_count: usize,
    cases: Vec<ReviewCaseStatusResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectQuery {
    project_id: String,
}

#[utoipa::path(post, path = "/case-review", tag = "case", request_body = CreateReviewRequest, responses((status = 201, body = CreatedReview)), security(("bearer" = [])))]
async fn create_review(
    user: AuthUser,
    State(st): State<ReviewState>,
    Json(req): Json<CreateReviewRequest>,
) -> Response {
    if !user.can("CASE_REVIEW", "REVIEW") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let rule = if req.pass_rule.trim().is_empty() { "SINGLE" } else { req.pass_rule.trim() };
    let new_review = NewReview {
        project_id: req.project_id,
        pass_rule: rule.to_string(),
        reviewer_count: req.reviewer_count.max(1),
        case_ids: req.case_ids,
        created_by: user.user_id,
        meta: ReviewMeta {
            name: req.name.trim().to_string(),
            description: req.description,
            tags: req.tags,
            module_id: req.module_id,
            start_at: req.start_at,
            end_at: req.end_at,
        },
    };
    match st.repo.create_review(&new_review).await {
        Ok(id) => {
            // Reviewers are not assigned up front: candidate reviewers = project
            // members (falls back to the creator when the project has no member list).
            if let Some(notifier) = &st.notifier {
                let title = if new_review.meta.name.is_empty() {
                    id.clone()
                } else {
                    new_review.meta.name.clone()
                };
                notifier
                    .notify_project_members(
                        &new_review.created_by,
                        NoticeEvent {
                            project_id: new_review.project_id.clone(),
                            category: "CASE".into(),
                            event_type: "REVIEW_CREATED".into(),
                            title,
                            content: new_review.case_ids.len().to_string(),
                            resource_type: "CASE_REVIEW".into(),
                            resource_id: id.clone(),
                            operator: new_review.created_by.clone(),
                        },
                    )
                    .await;
            }
            (StatusCode::CREATED, Json(CreatedReview { id })).into_response()
        }
        Err(e) => repo_err(e),
    }
}

#[utoipa::path(put, path = "/case-review/{review_id}", tag = "case", params(("review_id" = String, Path)), request_body = UpdateReviewRequest, responses((status = 200), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn update_review(
    user: AuthUser,
    State(st): State<ReviewState>,
    Path(review_id): Path<String>,
    Json(req): Json<UpdateReviewRequest>,
) -> Response {
    if !user.can("CASE_REVIEW", "REVIEW") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let update = ReviewMetaUpdate {
        pass_rule: req.pass_rule,
        reviewer_count: req.reviewer_count,
        meta: ReviewMeta {
            name: req.name,
            description: req.description,
            tags: req.tags,
            module_id: req.module_id,
            start_at: req.start_at,
            end_at: req.end_at,
        },
    };
    match st.update_meta.execute(&review_id, update).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(UpdateReviewMetaError::Validation(msg)) => {
            (StatusCode::BAD_REQUEST, msg).into_response()
        }
        Err(UpdateReviewMetaError::Repo(e)) => repo_err(e),
    }
}

#[utoipa::path(delete, path = "/case-review/{review_id}", tag = "case", params(("review_id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn delete_review_handler(
    user: AuthUser,
    State(st): State<ReviewState>,
    Path(review_id): Path<String>,
) -> Response {
    if !user.can("CASE_REVIEW", "REVIEW") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.delete_review.execute(&review_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(DeleteReviewError::Repo(e)) => repo_err(e),
    }
}

#[utoipa::path(get, path = "/case-review", tag = "case", params(("projectId" = String, Query)), responses((status = 200, body = [ReviewSummaryResponse])), security(("bearer" = [])))]
async fn list_reviews(
    user: AuthUser,
    State(st): State<ReviewState>,
    Query(q): Query<ProjectQuery>,
) -> Response {
    if !user.can("CASE_REVIEW", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.repo.list_reviews(&q.project_id).await {
        Ok(rs) => {
            let items: Vec<ReviewSummaryResponse> = rs
                .into_iter()
                .map(|r| ReviewSummaryResponse {
                    id: r.id,
                    pass_rule: r.pass_rule,
                    reviewer_count: r.reviewer_count,
                    total: r.total,
                    passed: r.passed,
                    created_at: r.created_at,
                    status: r.status,
                    name: r.meta.name,
                    description: r.meta.description,
                    tags: r.meta.tags,
                    module_id: r.meta.module_id,
                    start_at: r.meta.start_at,
                    end_at: r.meta.end_at,
                    created_by: r.created_by,
                    reviewers: r.reviewers,
                })
                .collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(e) => repo_err(e),
    }
}

#[utoipa::path(get, path = "/case-review/{review_id}", tag = "case", params(("review_id" = String, Path)), responses((status = 200, body = ReviewDetailResponse), (status = 404)), security(("bearer" = [])))]
async fn get_review(
    user: AuthUser,
    State(st): State<ReviewState>,
    Path(review_id): Path<String>,
) -> Response {
    if !user.can("CASE_REVIEW", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.repo.get_review(&review_id).await {
        Ok(d) => (
            StatusCode::OK,
            Json(ReviewDetailResponse {
                id: d.id,
                pass_rule: d.pass_rule,
                reviewer_count: d.reviewer_count,
                cases: d
                    .cases
                    .into_iter()
                    .map(|c| ReviewCaseStatusResponse { case_id: c.case_id, status: c.status })
                    .collect(),
            }),
        )
            .into_response(),
        Err(e) => repo_err(e),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct SubmitRequest {
    reviewer_id: String,
    status: String,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
struct SubmitResponse {
    status: String,
}

#[utoipa::path(post, path = "/case-review/{review_id}/{case_id}", tag = "case", params(("review_id" = String, Path), ("case_id" = String, Path)), request_body = SubmitRequest, responses((status = 200, body = SubmitResponse), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn submit_review(
    user: AuthUser,
    State(st): State<ReviewState>,
    Path((review_id, case_id)): Path<(String, String)>,
    Json(req): Json<SubmitRequest>,
) -> Response {
    if !user.can("CASE_REVIEW", "REVIEW") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let Some(status) = ReviewStatus::parse(&req.status) else {
        return (StatusCode::BAD_REQUEST, "unknown review status").into_response();
    };
    let verdict = match Verdict::new(&req.reviewer_id, status, req.content.as_deref()) {
        Ok(v) => v,
        Err(ReviewError::ContentRequiredForUnPass) => {
            return (StatusCode::BAD_REQUEST, "content required for UN_PASS").into_response();
        }
    };
    match st.use_case.execute(&review_id, &case_id, verdict).await {
        Ok(aggregated) => {
            (StatusCode::OK, Json(SubmitResponse { status: aggregated.as_str().to_string() }))
                .into_response()
        }
        Err(SubmitReviewError::Repo(RepoError::NotFound)) => {
            (StatusCode::NOT_FOUND, "review not found").into_response()
        }
        Err(SubmitReviewError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid verdict").into_response()
        }
        Err(SubmitReviewError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(OpenApi)]
#[openapi(paths(submit_review, create_review, update_review, list_reviews, get_review, delete_review_handler), components(schemas(SubmitRequest, SubmitResponse, CreateReviewRequest, UpdateReviewRequest, CreatedReview, ReviewSummaryResponse, ReviewCaseStatusResponse, ReviewDetailResponse)), tags((name = "case", description = "用例评审")))]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryReviewRepository;
    use crate::domain::{PassRule, ReviewSetting};
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use std::sync::Arc;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app_with(rule: PassRule, reviewer_count: usize) -> (Router, String) {
        let repo = Arc::new(InMemoryReviewRepository::new());
        repo.set_setting("rev1", ReviewSetting { rule, reviewer_count });
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms =
            PermissionSet::from_raw(["CASE_REVIEW:READ+REVIEW".to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        let uc = SubmitReviewUseCase::new(repo.clone());
        (router(uc, repo, None, sessions), token)
    }

    fn post(review: &str, case: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder()
            .method("POST")
            .uri(format!("/case-review/{review}/{case}"))
            .header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    #[tokio::test]
    async fn single_pass_returns_200_pass() {
        let (app, t) = app_with(PassRule::Single, 1).await;
        let resp = app
            .oneshot(post("rev1", "c1", r#"{"reviewerId":"u1","status":"PASS"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["status"], "PASS");
    }

    #[tokio::test]
    async fn submit_without_token_401() {
        let (app, _t) = app_with(PassRule::Single, 1).await;
        let resp = app
            .oneshot(post("rev1", "c1", r#"{"reviewerId":"u1","status":"PASS"}"#, None))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn submit_without_permission_403() {
        let repo = Arc::new(InMemoryReviewRepository::new());
        repo.set_setting("rev1", ReviewSetting { rule: PassRule::Single, reviewer_count: 1 });
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["CASE_REVIEW:READ".to_string()]).expect("perms");
        let token = sessions.create("v", perms, 3600).await.expect("token");
        let app = router(SubmitReviewUseCase::new(repo.clone()), repo, None, sessions);
        let resp = app
            .oneshot(post("rev1", "c1", r#"{"reviewerId":"u1","status":"PASS"}"#, Some(&token)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn multiple_first_pass_returns_under_reviewed() {
        let (app, t) = app_with(PassRule::Multiple, 2).await;
        let resp = app
            .oneshot(post("rev1", "c1", r#"{"reviewerId":"u1","status":"PASS"}"#, Some(&t)))
            .await
            .expect("resp");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["status"], "UNDER_REVIEWED");
    }

    #[tokio::test]
    async fn un_pass_without_content_returns_400() {
        let (app, t) = app_with(PassRule::Single, 1).await;
        let resp = app
            .oneshot(post("rev1", "c1", r#"{"reviewerId":"u1","status":"UN_PASS"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_status_returns_400() {
        let (app, t) = app_with(PassRule::Single, 1).await;
        let resp = app
            .oneshot(post("rev1", "c1", r#"{"reviewerId":"u1","status":"WAT"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn missing_review_setting_returns_404() {
        let (app, t) = app_with(PassRule::Single, 1).await;
        let resp = app
            .oneshot(post("nope", "c1", r#"{"reviewerId":"u1","status":"PASS"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    fn json_req(method: &str, uri: &str, body: &str, token: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body.to_string()))
            .expect("req")
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    #[tokio::test]
    async fn create_with_meta_then_list_returns_meta_and_put_edits_it() {
        let (app, t) = app_with(PassRule::Single, 1).await;

        let create_body = r#"{"projectId":"p1","passRule":"SINGLE","reviewerCount":1,
            "caseIds":["c1","c2"],"name":"v1.0 版本用例评审","description":"首轮",
            "tags":["冒烟"],"moduleId":"m1","startAt":"2026-07-01T00:00:00Z","endAt":"2026-07-31T00:00:00Z"}"#;
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/case-review", create_body, &t))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let created = body_json(resp).await;
        let id = created["id"].as_str().expect("id").to_string();

        let resp = app
            .clone()
            .oneshot(json_req("GET", "/case-review?projectId=p1", "", &t))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let list = body_json(resp).await;
        let item = &list.as_array().expect("array")[0];
        assert_eq!(item["name"], "v1.0 版本用例评审");
        assert_eq!(item["description"], "首轮");
        assert_eq!(item["tags"][0], "冒烟");
        assert_eq!(item["moduleId"], "m1");
        assert_eq!(item["status"], "IN_PROGRESS");
        assert_eq!(item["total"], 2);
        assert_eq!(item["createdBy"], "admin");

        let put_body = r#"{"name":"v2 评审","description":"二轮","tags":["回归"],
            "moduleId":null,"passRule":"MULTIPLE","reviewerCount":2}"#;
        let resp = app
            .clone()
            .oneshot(json_req("PUT", &format!("/case-review/{id}"), put_body, &t))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);

        let resp =
            app.oneshot(json_req("GET", "/case-review?projectId=p1", "", &t)).await.expect("resp");
        let list = body_json(resp).await;
        let item = &list.as_array().expect("array")[0];
        assert_eq!(item["name"], "v2 评审");
        assert_eq!(item["passRule"], "MULTIPLE");
        assert_eq!(item["reviewerCount"], 2);
        assert!(item["moduleId"].is_null());
    }

    #[tokio::test]
    async fn delete_review_round_trip_204_then_404_and_gone_from_list() {
        let (app, t) = app_with(PassRule::Single, 1).await;
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/case-review", r#"{"projectId":"p1","name":"n"}"#, &t))
            .await
            .expect("resp");
        let id = body_json(resp).await["id"].as_str().expect("id").to_string();

        let resp = app
            .clone()
            .oneshot(json_req("DELETE", &format!("/case-review/{id}"), "", &t))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let resp = app
            .clone()
            .oneshot(json_req("GET", &format!("/case-review/{id}"), "", &t))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let resp = app
            .clone()
            .oneshot(json_req("GET", "/case-review?projectId=p1", "", &t))
            .await
            .expect("resp");
        let list = body_json(resp).await;
        assert!(list.as_array().expect("array").is_empty());

        // Second delete of the same id is a miss.
        let resp = app
            .oneshot(json_req("DELETE", &format!("/case-review/{id}"), "", &t))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_missing_review_returns_404() {
        let (app, t) = app_with(PassRule::Single, 1).await;
        let resp =
            app.oneshot(json_req("DELETE", "/case-review/nope", "", &t)).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_without_review_permission_returns_403() {
        let repo = Arc::new(InMemoryReviewRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["CASE_REVIEW:READ".to_string()]).expect("perms");
        let token = sessions.create("v", perms, 3600).await.expect("token");
        let app = router(SubmitReviewUseCase::new(repo.clone()), repo, None, sessions);
        let resp =
            app.oneshot(json_req("DELETE", "/case-review/rev1", "", &token)).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn put_blank_name_returns_400() {
        let (app, t) = app_with(PassRule::Single, 1).await;
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/case-review", r#"{"projectId":"p1","name":"n"}"#, &t))
            .await
            .expect("resp");
        let id = body_json(resp).await["id"].as_str().expect("id").to_string();
        let resp = app
            .oneshot(json_req("PUT", &format!("/case-review/{id}"), r#"{"name":"  "}"#, &t))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_missing_review_returns_404() {
        let (app, t) = app_with(PassRule::Single, 1).await;
        let resp = app
            .oneshot(json_req("PUT", "/case-review/nope", r#"{"name":"n"}"#, &t))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_without_review_permission_returns_403() {
        let repo = Arc::new(InMemoryReviewRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["CASE_REVIEW:READ".to_string()]).expect("perms");
        let token = sessions.create("v", perms, 3600).await.expect("token");
        let app = router(SubmitReviewUseCase::new(repo.clone()), repo, None, sessions);
        let resp = app
            .oneshot(json_req("PUT", "/case-review/rev1", r#"{"name":"n"}"#, &token))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
