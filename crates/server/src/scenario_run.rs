//! 组装根桥:`POST /api/scenario/{id}/run`。
//!
//! 把「场景编译」(api-scenario)与「批量执行」(api-test)在组装根接起来——
//! 编译场景为可运行步骤,取其 case_id 交给现成的批量运行用例派发。两个有界上下文
//! 互不依赖,跨域协调只发生在这里(与 orchestration.rs 同构)。
//!
//! 说明:本轮只派发引用了接口用例(CASE 步骤)的可运行步骤;纯内联请求(REQUEST)
//! 步骤尚未物化为 ms_api_case,故暂不进入批量运行,留待后续。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use api_scenario::application::{
    CompileError, CompileScenarioUseCase, RecordScenarioExecutionUseCase,
};
use api_test::application::StartBatchRunUseCase;
use api_test::domain::{BatchRunError, BatchRunMode, RunModeConfig};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct RunState {
    compile: CompileScenarioUseCase,
    batch: StartBatchRunUseCase,
    recorder: RecordScenarioExecutionUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<RunState> for Arc<dyn SessionStore> {
    fn from_ref(s: &RunState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    compile: CompileScenarioUseCase,
    batch: StartBatchRunUseCase,
    recorder: RecordScenarioExecutionUseCase,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/api/scenario/{id}/run", post(run_scenario))
        .with_state(RunState { compile, batch, recorder, sessions })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunScenarioBody {
    project_id: String,
    #[serde(default = "default_mode")]
    run_mode: String,
    #[serde(default)]
    pool_id: Option<String>,
    /// 运行所用环境 id(注入 base_url/默认头/变量);缺省不注入。
    #[serde(default)]
    environment_id: Option<String>,
}

fn default_mode() -> String {
    "PARALLEL".to_string()
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunScenarioResponse {
    report_id: String,
    status: String,
    case_count: usize,
}

#[utoipa::path(
    post,
    path = "/api/scenario/{id}/run",
    tag = "api-scenario",
    params(("id" = String, Path)),
    request_body = RunScenarioBody,
    responses(
        (status = 200, body = RunScenarioResponse),
        (status = 400),
        (status = 404),
        (status = 409)
    ),
    security(("bearer" = []))
)]
async fn run_scenario(
    user: AuthUser,
    State(st): State<RunState>,
    Path(id): Path<String>,
    Json(req): Json<RunScenarioBody>,
) -> Response {
    if !user.can("API_SCENARIO", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    // 1) 编译场景为可运行步骤(递归展开子场景,环/深度在用例内拦)
    let steps = match st.compile.execute(&id).await {
        Ok(s) => s,
        Err(CompileError::NotFound(_)) => {
            return (StatusCode::NOT_FOUND, "scenario not found").into_response();
        }
        Err(CompileError::Cycle(_)) | Err(CompileError::Depth(_)) => {
            return (StatusCode::CONFLICT, "scenario reference cycle or too deep").into_response();
        }
        Err(CompileError::Repo(_)) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
        }
    };
    // 2) 取引用了用例的步骤的 case_id(内联请求步骤本轮跳过)
    let case_ids: Vec<String> = steps.into_iter().filter_map(|s| s.case_id).collect();
    if case_ids.is_empty() {
        return (StatusCode::BAD_REQUEST, "scenario has no runnable cases (CASE steps)")
            .into_response();
    }
    let count = case_ids.len();
    // 3) 解析运行模式,交批量运行用例派发(复用资源池解析/可用性规则)
    let Some(mode) = BatchRunMode::parse(&req.run_mode) else {
        return (StatusCode::BAD_REQUEST, "unknown run mode").into_response();
    };
    let config =
        RunModeConfig { mode, pool_id: req.pool_id, retry: None, environment_id: req.environment_id };
    match st.batch.execute(&req.project_id, case_ids, config).await {
        Ok(rep) => {
            // 闭环:用批量运行回传的真实状态落场景执行记录——同步 runner 跑完即
            // SUCCESS/ERROR,异步执行器为 RUNNING(后续由执行节点回写)。记录失败不影响运行结果。
            let _ = st
                .recorder
                .execute(&id, &req.project_id, &rep.status, count as i32, Some(&rep.report_id))
                .await;
            (
                StatusCode::OK,
                Json(RunScenarioResponse {
                    report_id: rep.report_id,
                    status: rep.status,
                    case_count: count,
                }),
            )
                .into_response()
        }
        Err(BatchRunError::ResourcePoolNotConfigured) => (
            StatusCode::BAD_REQUEST,
            "resource pool not configured (supply poolId or set project default)",
        )
            .into_response(),
        Err(BatchRunError::ResourcePoolUnavailable { pool_id }) => {
            (StatusCode::CONFLICT, format!("resource pool unavailable: {pool_id}")).into_response()
        }
        Err(BatchRunError::NoCases) => {
            (StatusCode::BAD_REQUEST, "no cases to run").into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "dispatch error").into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(run_scenario),
    components(schemas(RunScenarioBody, RunScenarioResponse)),
    tags((name = "api-scenario", description = "场景"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
