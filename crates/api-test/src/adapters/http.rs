//! 接口测试批量运行的 HTTP 适配器:`POST /api/batch-run`。
//!
//! 错误码映射体现 quirk 的两种失败:**未配置池 → 400**(客户端该传/项目该配),
//! **池不可用 → 409**(引用了但不可用)。两者都在入口返回,而非下游 500。

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use crate::application::StartBatchRunUseCase;
use crate::domain::{BatchRunError, BatchRunMode, RunModeConfig};
use serde::{Deserialize, Serialize};

pub fn router(use_case: StartBatchRunUseCase) -> Router {
    Router::new().route("/api/batch-run", post(batch_run)).with_state(use_case)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRunRequest {
    project_id: String,
    case_ids: Vec<String>,
    run_mode: String,
    #[serde(default)]
    pool_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchRunResponse {
    report_id: String,
}

async fn batch_run(
    State(uc): State<StartBatchRunUseCase>,
    Json(req): Json<BatchRunRequest>,
) -> Response {
    let Some(mode) = BatchRunMode::parse(&req.run_mode) else {
        return (StatusCode::BAD_REQUEST, "unknown run mode").into_response();
    };
    let config = RunModeConfig { mode, pool_id: req.pool_id, retry: None };

    match uc.execute(&req.project_id, req.case_ids, config).await {
        Ok(report_id) => (StatusCode::OK, Json(BatchRunResponse { report_id })).into_response(),
        Err(BatchRunError::NoCases) => (StatusCode::BAD_REQUEST, "no cases to run").into_response(),
        Err(BatchRunError::InvalidRetryConfig) => {
            (StatusCode::BAD_REQUEST, "invalid retry config").into_response()
        }
        Err(BatchRunError::ResourcePoolNotConfigured) => {
            (StatusCode::BAD_REQUEST, "resource pool not configured (supply poolId or set project default)")
                .into_response()
        }
        Err(BatchRunError::ResourcePoolUnavailable { pool_id }) => {
            (StatusCode::CONFLICT, format!("resource pool unavailable: {pool_id}")).into_response()
        }
        Err(BatchRunError::Backend(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use crate::adapters::{FakeResourcePool, SpyExecutor};
    use std::sync::Arc;
    use tower::ServiceExt;

    fn app(pools: FakeResourcePool) -> Router {
        let uc = StartBatchRunUseCase::new(Arc::new(pools), Arc::new(SpyExecutor::new()));
        router(uc)
    }

    fn post(body: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/batch-run")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("req")
    }

    #[tokio::test]
    async fn client_pool_available_returns_200_with_report() {
        let pools = FakeResourcePool::new().with_available("pool1");
        let resp = app(pools)
            .oneshot(post(r#"{"projectId":"p1","caseIds":["c1"],"runMode":"PARALLEL","poolId":"pool1"}"#))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(v["reportId"].as_str().expect("id").starts_with("report-"));
    }

    #[tokio::test]
    async fn no_pool_configured_returns_400() {
        // 项目无默认池 + 客户端未传:原 500 场景 → 这里 400
        let resp = app(FakeResourcePool::new())
            .oneshot(post(r#"{"projectId":"p1","caseIds":["c1"],"runMode":"PARALLEL"}"#))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unavailable_pool_returns_409() {
        let resp = app(FakeResourcePool::new())
            .oneshot(post(r#"{"projectId":"p1","caseIds":["c1"],"runMode":"PARALLEL","poolId":"dead"}"#))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn empty_cases_returns_400() {
        let pools = FakeResourcePool::new().with_available("pool1");
        let resp = app(pools)
            .oneshot(post(r#"{"projectId":"p1","caseIds":[],"runMode":"PARALLEL","poolId":"pool1"}"#))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_run_mode_returns_400() {
        let pools = FakeResourcePool::new().with_available("pool1");
        let resp = app(pools)
            .oneshot(post(r#"{"projectId":"p1","caseIds":["c1"],"runMode":"WAT","poolId":"pool1"}"#))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
