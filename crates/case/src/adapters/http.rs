//! 用例评审的 HTTP 适配器:`POST /case-review/{reviewId}/{caseId}` 提交一次评审。
//!
//! 只做翻译:解析状态串/DTO → 构造已校验的 `Verdict` → 调用例 → 映射错误码。
//! 聚合与状态机规则都在 domain/application,本层不碰。
//! 写端点(提交评审)经 `webauth::AuthUser` 做 RBAC:需 `CASE_REVIEW:REVIEW`。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use crate::application::{SubmitReviewError, SubmitReviewUseCase};
use crate::domain::{ReviewError, ReviewStatus, Verdict};
use crate::ports::{RepoError, ReviewRepository};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct ReviewState {
    use_case: SubmitReviewUseCase,
    repo: Arc<dyn ReviewRepository>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<ReviewState> for Arc<dyn SessionStore> {
    fn from_ref(s: &ReviewState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(use_case: SubmitReviewUseCase, repo: Arc<dyn ReviewRepository>, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/case-review", post(create_review).get(list_reviews))
        .route("/case-review/{review_id}", get(get_review))
        .route("/case-review/{review_id}/{case_id}", post(submit_review))
        .with_state(ReviewState { use_case, repo, sessions })
}

fn repo_err(e: RepoError) -> Response {
    match e {
        RepoError::NotFound => (StatusCode::NOT_FOUND, "review not found").into_response(),
        RepoError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CreateReviewRequest {
    project_id: String,
    /// SINGLE(或签)/ MULTIPLE(会签)。
    #[serde(default)]
    pass_rule: String,
    #[serde(default = "one")]
    reviewer_count: usize,
    #[serde(default)]
    case_ids: Vec<String>,
}
fn one() -> usize { 1 }

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
async fn create_review(user: AuthUser, State(st): State<ReviewState>, Json(req): Json<CreateReviewRequest>) -> Response {
    if !user.can("CASE_REVIEW", "REVIEW") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let rule = if req.pass_rule.trim().is_empty() { "SINGLE" } else { req.pass_rule.trim() };
    match st.repo.create_review(&req.project_id, rule, req.reviewer_count.max(1), &req.case_ids).await {
        Ok(id) => (StatusCode::CREATED, Json(CreatedReview { id })).into_response(),
        Err(e) => repo_err(e),
    }
}

#[utoipa::path(get, path = "/case-review", tag = "case", params(("projectId" = String, Query)), responses((status = 200, body = [ReviewSummaryResponse])))]
async fn list_reviews(State(st): State<ReviewState>, Query(q): Query<ProjectQuery>) -> Response {
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
                })
                .collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(e) => repo_err(e),
    }
}

#[utoipa::path(get, path = "/case-review/{review_id}", tag = "case", params(("review_id" = String, Path)), responses((status = 200, body = ReviewDetailResponse), (status = 404)))]
async fn get_review(State(st): State<ReviewState>, Path(review_id): Path<String>) -> Response {
    match st.repo.get_review(&review_id).await {
        Ok(d) => (
            StatusCode::OK,
            Json(ReviewDetailResponse {
                id: d.id,
                pass_rule: d.pass_rule,
                reviewer_count: d.reviewer_count,
                cases: d.cases.into_iter().map(|c| ReviewCaseStatusResponse { case_id: c.case_id, status: c.status }).collect(),
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
    // 1) 解析状态串
    let Some(status) = ReviewStatus::parse(&req.status) else {
        return (StatusCode::BAD_REQUEST, "unknown review status").into_response();
    };
    // 2) 构造已校验的 Verdict(UnPass 须带评论)
    let verdict = match Verdict::new(&req.reviewer_id, status, req.content.as_deref()) {
        Ok(v) => v,
        Err(ReviewError::ContentRequiredForUnPass) => {
            return (StatusCode::BAD_REQUEST, "content required for UN_PASS").into_response();
        }
    };
    // 3) 调用例,映射结果
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
#[openapi(paths(submit_review, create_review, list_reviews, get_review), components(schemas(SubmitRequest, SubmitResponse, CreateReviewRequest, CreatedReview, ReviewSummaryResponse, ReviewCaseStatusResponse, ReviewDetailResponse)), tags((name = "case", description = "用例评审")))]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi { ApiDoc::openapi() }

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use crate::adapters::InMemoryReviewRepository;
    use crate::domain::{PassRule, ReviewSetting};
    use kernel::permission::PermissionSet;
    use std::sync::Arc;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    /// app + 拥有 `CASE_REVIEW:READ+REVIEW` 的令牌。
    async fn app_with(rule: PassRule, reviewer_count: usize) -> (Router, String) {
        let repo = Arc::new(InMemoryReviewRepository::new());
        repo.set_setting("rev1", ReviewSetting { rule, reviewer_count });
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["CASE_REVIEW:READ+REVIEW".to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        let uc = SubmitReviewUseCase::new(repo.clone());
        (router(uc, repo, sessions), token)
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
        let app = router(SubmitReviewUseCase::new(repo.clone()), repo, sessions);
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
}
