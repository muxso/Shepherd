//! 用例评审的 HTTP 适配器:`POST /case-review/{reviewId}/{caseId}` 提交一次评审。
//!
//! 只做翻译:解析状态串/DTO → 构造已校验的 `Verdict` → 调用例 → 映射错误码。
//! 聚合与状态机规则都在 domain/application,本层不碰。
//! 写端点(提交评审)经 `webauth::AuthUser` 做 RBAC:需 `CASE_REVIEW:REVIEW`。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use crate::application::{SubmitReviewError, SubmitReviewUseCase};
use crate::domain::{ReviewError, ReviewStatus, Verdict};
use crate::ports::RepoError;
use serde::{Deserialize, Serialize};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct ReviewState {
    use_case: SubmitReviewUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<ReviewState> for Arc<dyn SessionStore> {
    fn from_ref(s: &ReviewState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(use_case: SubmitReviewUseCase, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/case-review/{review_id}/{case_id}", post(submit_review))
        .with_state(ReviewState { use_case, sessions })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubmitRequest {
    reviewer_id: String,
    status: String,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Serialize)]
struct SubmitResponse {
    status: String,
}

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
        let repo = InMemoryReviewRepository::new();
        repo.set_setting("rev1", ReviewSetting { rule, reviewer_count });
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["CASE_REVIEW:READ+REVIEW".to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        let uc = SubmitReviewUseCase::new(Arc::new(repo));
        (router(uc, sessions), token)
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
        let repo = InMemoryReviewRepository::new();
        repo.set_setting("rev1", ReviewSetting { rule: PassRule::Single, reviewer_count: 1 });
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["CASE_REVIEW:READ".to_string()]).expect("perms");
        let token = sessions.create("v", perms, 3600).await.expect("token");
        let app = router(SubmitReviewUseCase::new(Arc::new(repo)), sessions);
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
