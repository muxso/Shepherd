//! 测试计划的 HTTP 适配器:`POST /test-plan`(创建)、`GET /test-plan/{id}/statistics`(统计)。
//!
//! 写端点(创建)经 `webauth::AuthUser` 做 RBAC:需 `TEST_PLAN:ADD`;统计为读端点,不设限。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use crate::application::{
    CreatePlanError, CreatePlanUseCase, PlanStatisticsError, PlanStatisticsUseCase,
};
use crate::domain::{Plan, PlanType, ROOT_GROUP};
use serde::{Deserialize, Serialize};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct PlanState {
    create: CreatePlanUseCase,
    stats: PlanStatisticsUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<PlanState> for Arc<dyn SessionStore> {
    fn from_ref(s: &PlanState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    create: CreatePlanUseCase,
    stats: PlanStatisticsUseCase,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/test-plan", post(create_plan))
        .route("/test-plan/{id}/statistics", get(statistics))
        .with_state(PlanState { create, stats, sessions })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePlanRequest {
    project_id: String,
    name: String,
    #[serde(rename = "type")]
    plan_type: String,
    #[serde(default = "default_group")]
    group_id: String,
}

fn default_group() -> String {
    ROOT_GROUP.to_string()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanResponse {
    id: String,
    project_id: String,
    name: String,
    #[serde(rename = "type")]
    plan_type: String,
    group_id: String,
}

impl From<Plan> for PlanResponse {
    fn from(p: Plan) -> Self {
        Self {
            id: p.id,
            project_id: p.project_id,
            name: p.name,
            plan_type: p.plan_type.as_str().to_string(),
            group_id: p.group_id,
        }
    }
}

async fn create_plan(
    user: AuthUser,
    State(st): State<PlanState>,
    Json(req): Json<CreatePlanRequest>,
) -> Response {
    if !user.can("TEST_PLAN", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let Some(plan_type) = PlanType::parse(&req.plan_type) else {
        return (StatusCode::BAD_REQUEST, "unknown plan type").into_response();
    };
    match st.create.execute(&req.project_id, &req.name, plan_type, &req.group_id).await {
        Ok(p) => (StatusCode::CREATED, Json(PlanResponse::from(p))).into_response(),
        Err(CreatePlanError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid plan payload").into_response()
        }
        Err(CreatePlanError::InvalidGroup) => {
            (StatusCode::BAD_REQUEST, "group not found or not a group").into_response()
        }
        Err(CreatePlanError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatisticsResponse {
    status: String,
    total: u64,
    pass_rate: f64,
    execute_rate: f64,
    is_pass: bool,
}

async fn statistics(State(st): State<PlanState>, Path(id): Path<String>) -> Response {
    match st.stats.execute(&id).await {
        Ok(s) => (
            StatusCode::OK,
            Json(StatisticsResponse {
                status: s.status.as_str().to_string(),
                total: s.total,
                pass_rate: s.pass_rate,
                execute_rate: s.execute_rate,
                is_pass: s.is_pass,
            }),
        )
            .into_response(),
        Err(PlanStatisticsError::PlanNotFound) => {
            (StatusCode::NOT_FOUND, "plan not found").into_response()
        }
        Err(PlanStatisticsError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use crate::adapters::InMemoryPlanRepository;
    use crate::domain::{CaseCounts, NewPlan};
    use kernel::permission::PermissionSet;
    use std::sync::Arc;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    /// app + 拥有 `TEST_PLAN:READ+ADD` 的令牌。
    async fn app_with(repo: InMemoryPlanRepository) -> (Router, String) {
        let repo = Arc::new(repo);
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["TEST_PLAN:READ+ADD".to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        let r = router(
            CreatePlanUseCase::new(repo.clone()),
            PlanStatisticsUseCase::new(repo),
            sessions,
        );
        (r, token)
    }

    fn post(uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method("POST").uri(uri).header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    #[tokio::test]
    async fn create_root_plan_201() {
        let (app, t) = app_with(InMemoryPlanRepository::new()).await;
        let resp = app
            .oneshot(post(
                "/test-plan",
                r#"{"projectId":"p1","name":"冒烟","type":"TEST_PLAN"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn create_without_token_401() {
        let (app, _t) = app_with(InMemoryPlanRepository::new()).await;
        let resp = app
            .oneshot(post("/test-plan", r#"{"projectId":"p1","name":"x","type":"TEST_PLAN"}"#, None))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_without_permission_403() {
        let repo = Arc::new(InMemoryPlanRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["TEST_PLAN:READ".to_string()]).expect("perms");
        let token = sessions.create("v", perms, 3600).await.expect("token");
        let app = router(
            CreatePlanUseCase::new(repo.clone()),
            PlanStatisticsUseCase::new(repo),
            sessions,
        );
        let resp = app
            .oneshot(post(
                "/test-plan",
                r#"{"projectId":"p1","name":"x","type":"TEST_PLAN"}"#,
                Some(&token),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn create_nested_group_400() {
        let (app, t) = app_with(InMemoryPlanRepository::new()).await;
        let resp = app
            .oneshot(post(
                "/test-plan",
                r#"{"projectId":"p1","name":"组中组","type":"GROUP","groupId":"g1"}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn create_unknown_type_400() {
        let (app, t) = app_with(InMemoryPlanRepository::new()).await;
        let resp = app
            .oneshot(post("/test-plan", r#"{"projectId":"p1","name":"x","type":"WAT"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn statistics_returns_rates() {
        let repo = InMemoryPlanRepository::new();
        let plan = repo
            .seed(NewPlan::new("p1", "冒烟", PlanType::Plan, ROOT_GROUP).expect("v"))
            .await;
        repo.set_counts(&plan.id, CaseCounts { pending: 1, success: 2, error: 1, ..Default::default() });
        repo.set_threshold(&plan.id, 0.5);

        let (app, _t) = app_with(repo).await;
        // 读端点,无需令牌
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/test-plan/{}/statistics", plan.id))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["status"], "UNDERWAY");
        assert_eq!(v["total"], 4);
        assert_eq!(v["isPass"], true);
    }

    #[tokio::test]
    async fn statistics_missing_plan_404() {
        let (app, _t) = app_with(InMemoryPlanRepository::new()).await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/test-plan/ghost/statistics")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
