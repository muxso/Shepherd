//! 接口测试批量运行的 HTTP 适配器:`POST /api/batch-run`。
//!
//! 错误码映射体现 quirk 的两种失败:**未配置池 → 400**(客户端该传/项目该配),
//! **池不可用 → 409**(引用了但不可用)。两者都在入口返回,而非下游 500。

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use crate::application::{ListCaseExecutionsUseCase, StartBatchRunUseCase};
use crate::domain::{BatchRunError, BatchRunMode, RunModeConfig};
use crate::ports::CaseExecutionRecord;
use kernel::page::PageRequest;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};

pub fn router(use_case: StartBatchRunUseCase) -> Router {
    Router::new().route("/api/batch-run", post(batch_run)).with_state(use_case)
}

/// 用例执行记录(用例执行记录)分页查询的只读路由——开放(无鉴权)。
/// 不改动 `router(StartBatchRunUseCase)` 的既有签名,单独挂一条 GET。
pub fn executions_router(uc: ListCaseExecutionsUseCase) -> Router {
    Router::new()
        .route("/api/case/{caseId}/executions", get(list_executions))
        .with_state(uc)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct BatchRunRequest {
    project_id: String,
    case_ids: Vec<String>,
    run_mode: String,
    #[serde(default)]
    pool_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct BatchRunResponse {
    report_id: String,
    status: String,
}

#[utoipa::path(post, path = "/api/batch-run", tag = "api-test", request_body = BatchRunRequest, responses((status = 200, body = BatchRunResponse), (status = 400), (status = 409)))]
async fn batch_run(
    State(uc): State<StartBatchRunUseCase>,
    Json(req): Json<BatchRunRequest>,
) -> Response {
    let Some(mode) = BatchRunMode::parse(&req.run_mode) else {
        return (StatusCode::BAD_REQUEST, "unknown run mode").into_response();
    };
    let config = RunModeConfig { mode, pool_id: req.pool_id, retry: None };

    match uc.execute(&req.project_id, req.case_ids, config).await {
        Ok(rep) => (
            StatusCode::OK,
            Json(BatchRunResponse { report_id: rep.report_id, status: rep.status }),
        )
            .into_response(),
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

// ---- 用例执行记录分页查询(只读) ----

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CaseExecutionRecordDto {
    report_id: String,
    case_id: String,
    outcome: String,
    failures: serde_json::Value,
    executed_at: String,
}

impl From<CaseExecutionRecord> for CaseExecutionRecordDto {
    fn from(r: CaseExecutionRecord) -> Self {
        Self {
            report_id: r.report_id,
            case_id: r.case_id,
            outcome: r.outcome,
            failures: r.failures,
            executed_at: r.executed_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CaseExecutionPageResponse {
    total: u64,
    current: u32,
    page_size: u32,
    total_pages: u64,
    items: Vec<CaseExecutionRecordDto>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct CaseExecutionQuery {
    #[serde(default = "default_current")]
    current: u32,
    #[serde(default = "default_page_size")]
    page_size: u32,
}

fn default_current() -> u32 {
    1
}
fn default_page_size() -> u32 {
    10
}

#[utoipa::path(
    get,
    path = "/api/case/{caseId}/executions",
    tag = "api-test",
    params(("caseId" = String, Path, description = "用例 id"), CaseExecutionQuery),
    responses((status = 200, body = CaseExecutionPageResponse), (status = 400))
)]
async fn list_executions(
    State(uc): State<ListCaseExecutionsUseCase>,
    Path(case_id): Path<String>,
    Query(q): Query<CaseExecutionQuery>,
) -> Response {
    // 分页参数校验复用 kernel:非法参数 → 400(而非打到 DB 才炸)
    let page = match PageRequest::new(q.current, q.page_size) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid page params").into_response(),
    };
    match uc.execute(&case_id, page).await {
        Ok(page) => {
            let body = CaseExecutionPageResponse {
                total: page.total,
                current: page.current,
                page_size: page.page_size,
                total_pages: page.total_pages(),
                items: page.items.into_iter().map(CaseExecutionRecordDto::from).collect(),
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(batch_run, list_executions),
    components(schemas(
        BatchRunRequest,
        BatchRunResponse,
        CaseExecutionRecordDto,
        CaseExecutionPageResponse
    )),
    tags((name = "api-test", description = "接口批量执行"))
)]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi { ApiDoc::openapi() }

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

    // ---- 用例执行记录分页路由 ----
    use crate::ports::{CaseExecutionQueryPort, CaseExecutionRecord, PortError};
    use async_trait::async_trait;

    struct FakeQuery {
        total: u64,
    }

    #[async_trait]
    impl CaseExecutionQueryPort for FakeQuery {
        async fn count_by_case(&self, _case_id: &str) -> Result<u64, PortError> {
            Ok(self.total)
        }
        async fn list_by_case(
            &self,
            case_id: &str,
            offset: u64,
            limit: u32,
        ) -> Result<Vec<CaseExecutionRecord>, PortError> {
            let end = (offset + limit as u64).min(self.total);
            Ok((offset..end)
                .map(|i| CaseExecutionRecord {
                    report_id: format!("r{i}"),
                    case_id: case_id.to_string(),
                    outcome: "SUCCESS".into(),
                    failures: serde_json::json!([]),
                    executed_at: "2026-05-31T00:00:00Z".into(),
                })
                .collect())
        }
    }

    fn exec_app(total: u64) -> Router {
        executions_router(ListCaseExecutionsUseCase::new(Arc::new(FakeQuery { total })))
    }

    #[tokio::test]
    async fn executions_returns_paginated_body() {
        let resp = exec_app(3)
            .oneshot(
                Request::builder()
                    .uri("/api/case/c1/executions?current=1&pageSize=2")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["total"], 3);
        assert_eq!(v["current"], 1);
        assert_eq!(v["pageSize"], 2);
        assert_eq!(v["totalPages"], 2); // ceil(3/2)
        let items = v["items"].as_array().expect("arr");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["reportId"], "r0");
        assert_eq!(items[0]["caseId"], "c1");
        assert_eq!(items[0]["executedAt"], "2026-05-31T00:00:00Z");
        assert!(items[0]["failures"].is_array());
    }

    #[tokio::test]
    async fn executions_defaults_apply_without_query() {
        let resp = exec_app(0)
            .oneshot(
                Request::builder()
                    .uri("/api/case/c1/executions")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["current"], 1); // 默认
        assert_eq!(v["pageSize"], 10); // 默认
    }

    #[tokio::test]
    async fn executions_bad_page_params_returns_400() {
        let resp = exec_app(3)
            .oneshot(
                Request::builder()
                    .uri("/api/case/c1/executions?current=0&pageSize=10")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
