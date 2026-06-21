//! 组装根桥:`POST /decomposition/{id}/run` —— 按依赖图**并行**编排整张任务 DAG。
//!
//! 逐层推进:每轮挑出「依赖全部 Verified 的 Pending 任务」,**并发派发**(信号量限流);
//! 每个派发经交付观察者驱动 验证门 + 自纠正 + 推进任务。无新就绪任务即停。
//! 依赖未达成(上游 Failed)的任务留在 Pending = blocked。RBAC 资源键 `TASK:EXECUTE`。

use std::collections::HashSet;
use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use utoipa::{OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use delivery::application::DeliveryService;
use requirement::application::RequirementService;
use task::application::{TaskCmdError, TaskService};
use task::domain::TaskStatus;

#[derive(Clone)]
struct RunState {
    tasks: TaskService,
    delivery: DeliveryService,
    requirements: RequirementService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<RunState> for Arc<dyn SessionStore> {
    fn from_ref(s: &RunState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    tasks: TaskService,
    delivery: DeliveryService,
    requirements: RequirementService,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/decomposition/{id}/run", post(run_decomposition))
        .with_state(RunState { tasks, delivery, requirements, sessions })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunBody {
    /// 执行者:CLAUDE_CODE(默认)| CODEX。
    #[serde(default = "default_executor")]
    executor: String,
    /// 并发上限(默认 4,至少 1)。
    #[serde(default = "default_concurrency")]
    max_concurrency: usize,
}

fn default_executor() -> String {
    "CLAUDE_CODE".to_string()
}
fn default_concurrency() -> usize {
    4
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunResponse {
    decomposition_id: String,
    total: usize,
    verified: usize,
    failed: usize,
    /// 依赖未达成、始终未就绪的任务数(上游失败 → 阻塞)。
    blocked: usize,
    /// 调度轮数(DAG 层级数)。
    rounds: u32,
}

#[utoipa::path(
    post, path = "/decomposition/{id}/run", tag = "task",
    params(("id" = String, Path)),
    request_body = RunBody,
    responses((status = 200, body = RunResponse), (status = 403), (status = 404)),
    security(("bearer" = []))
)]
async fn run_decomposition(
    user: AuthUser,
    State(st): State<RunState>,
    Path(id): Path<String>,
    Json(body): Json<RunBody>,
) -> Response {
    if !user.can("TASK", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    // 先确认存在。
    let dec = match st.tasks.get(&id).await {
        Ok(d) => d,
        Err(TaskCmdError::DecompositionNotFound) => {
            return (StatusCode::NOT_FOUND, "decomposition not found").into_response()
        }
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    };
    let total = dec.tasks.len();
    let max = body.max_concurrency.max(1);
    let sem = Arc::new(Semaphore::new(max));
    let executor = body.executor;

    let mut rounds = 0u32;
    // 上限 = 任务数 + 1,防病态(无进展)死循环。
    let guard = total as u32 + 1;
    loop {
        let dec = match st.tasks.get(&id).await {
            Ok(d) => d,
            Err(_) => break,
        };
        let verified: HashSet<&str> = dec
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Verified)
            .map(|t| t.id.as_str())
            .collect();
        // 就绪:自身 Pending 且依赖全部 Verified。
        let ready: Vec<_> = dec
            .tasks
            .iter()
            .filter(|t| {
                t.status == TaskStatus::Pending
                    && t.dependencies.iter().all(|d| verified.contains(d.as_str()))
            })
            .cloned()
            .collect();
        if ready.is_empty() || rounds >= guard {
            break;
        }
        rounds += 1;

        // 本层并发派发(信号量限流)。每次派发经观察者驱动验证门 + 自纠正 + 推进任务。
        let mut set: JoinSet<()> = JoinSet::new();
        for t in ready {
            let delivery = st.delivery.clone();
            let sem = sem.clone();
            let did = id.clone();
            let exec = executor.clone();
            set.spawn(async move {
                let _permit = sem.acquire_owned().await;
                let _ = delivery
                    .dispatch(&did, &t.id, &t.title, &t.description, &t.acceptance_criteria, &exec, None, None)
                    .await;
            });
        }
        while set.join_next().await.is_some() {}
    }

    // 终态统计。
    let final_dec = st.tasks.get(&id).await.unwrap_or(dec);
    let verified = final_dec.tasks.iter().filter(|t| t.status == TaskStatus::Verified).count();
    let failed = final_dec.tasks.iter().filter(|t| t.status == TaskStatus::Failed).count();
    let blocked = total - verified - failed;

    // 全部任务验证通过 = 该需求验收完整性达成 → 自动把需求标记交付(Baselined→Delivered)。
    // best-effort:未定基线/已归档会被领域层拒绝,这里仅记日志不影响运行结果。
    if total > 0 && verified == total {
        if let Err(e) = st.requirements.deliver(&final_dec.requirement_id).await {
            tracing::warn!(requirement = %final_dec.requirement_id, "自动标记交付失败(可能未定基线): {e:?}");
        } else {
            tracing::info!(requirement = %final_dec.requirement_id, "需求已自动标记交付(DELIVERED)");
        }
    }

    (
        StatusCode::OK,
        Json(RunResponse { decomposition_id: id, total, verified, failed, blocked, rounds }),
    )
        .into_response()
}

#[derive(OpenApi)]
#[openapi(paths(run_decomposition), components(schemas(RunBody, RunResponse)), tags((name = "task", description = "任务编排")))]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
