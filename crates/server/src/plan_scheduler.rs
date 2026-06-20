//! 组装根桥:测试计划定时执行(tokio-cron-scheduler)。
//!
//! 给计划配 cron;后台调度器到点为计划**拍一份统计快照**(PlanRun)存档,可看通过率/执行率趋势。
//! 端点:POST /test-plan/{id}/schedule(登记 + 立即挂上 live cron)、GET /test-plan/{id}/runs(历史快照)。
//! 启动时加载所有启用计划并注册;新建的计划立即挂上,无需重启。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use migrate::PgPool;
use serde::{Deserialize, Serialize};
use tokio_cron_scheduler::{Job, JobScheduler};
use webauth::{AuthUser, SessionStore};

use test_plan::adapters::pg::{PgPlanRepository, PgScheduleStore};
use test_plan::application::{
    CreateScheduleError, CreateScheduleUseCase, PlanStatisticsUseCase, ScheduledRunUseCase,
};

#[derive(Clone)]
struct SchedState {
    create: CreateScheduleUseCase,
    run_uc: ScheduledRunUseCase,
    sched: Arc<JobScheduler>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<SchedState> for Arc<dyn SessionStore> {
    fn from_ref(s: &SchedState) -> Self {
        s.sessions.clone()
    }
}

/// 注册一个 cron job:到点调用 `ScheduledRunUseCase.execute(plan_id)`(尽力而为,出错只记日志)。
async fn register_job(sched: &JobScheduler, run_uc: &ScheduledRunUseCase, plan_id: &str, cron: &str) {
    let uc = run_uc.clone();
    let pid = plan_id.to_string();
    let job = Job::new_async(cron, move |_uuid, _l| {
        let uc = uc.clone();
        let pid = pid.clone();
        Box::pin(async move {
            match uc.execute(&pid).await {
                Ok(run) => tracing::info!(plan = %pid, status = %run.status, "scheduled plan run snapshot"),
                Err(e) => tracing::warn!(plan = %pid, "scheduled run failed: {e:?}"),
            }
        })
    });
    match job {
        Ok(j) => {
            if let Err(e) = sched.add(j).await {
                tracing::warn!(plan = %plan_id, "add cron job failed: {e}");
            }
        }
        Err(e) => tracing::warn!(plan = %plan_id, cron, "invalid cron: {e}"),
    }
}

/// 构建调度器路由:创建并启动 JobScheduler,加载已启用计划注册其 job,返回 axum 路由。
pub async fn build(
    pool: PgPool,
    sessions: Arc<dyn SessionStore>,
) -> Result<Router, Box<dyn std::error::Error>> {
    let plan_repo = Arc::new(PgPlanRepository::new(pool.clone()));
    let store = Arc::new(PgScheduleStore::new(pool.clone()));
    let create = CreateScheduleUseCase::new(store.clone());
    let run_uc = ScheduledRunUseCase::new(PlanStatisticsUseCase::new(plan_repo), store);

    let sched = JobScheduler::new().await?;
    // 启动时加载已启用计划。
    if let Ok(schedules) = create.list_enabled().await {
        for s in schedules {
            register_job(&sched, &run_uc, &s.plan_id, &s.cron).await;
        }
    }
    sched.start().await?;
    let sched = Arc::new(sched);

    Ok(Router::new()
        .route("/test-plan/{id}/schedule", post(create_schedule))
        .route("/test-plan/{id}/runs", get(list_runs))
        .with_state(SchedState { create, run_uc, sched, sessions }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleBody {
    cron: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleResponse {
    id: String,
    plan_id: String,
    cron: String,
    enabled: bool,
}

async fn create_schedule(
    user: AuthUser,
    State(st): State<SchedState>,
    Path(id): Path<String>,
    Json(b): Json<ScheduleBody>,
) -> Response {
    if !user.can("TEST_PLAN", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.create.execute(&id, &b.cron, true).await {
        Ok(s) => {
            // 立即挂上 live cron(无需重启)。
            register_job(&st.sched, &st.run_uc, &s.plan_id, &s.cron).await;
            (
                StatusCode::CREATED,
                Json(ScheduleResponse {
                    id: s.id,
                    plan_id: s.plan_id,
                    cron: s.cron,
                    enabled: s.enabled,
                }),
            )
                .into_response()
        }
        Err(CreateScheduleError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid schedule (plan id / cron)").into_response()
        }
        Err(CreateScheduleError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunResponse {
    id: String,
    plan_id: String,
    status: String,
    total: u64,
    pass_rate: f64,
    execute_rate: f64,
    triggered_at: String,
}

async fn list_runs(State(st): State<SchedState>, Path(id): Path<String>) -> Response {
    match st.run_uc.list_runs(&id, 50).await {
        Ok(runs) => {
            let items: Vec<RunResponse> = runs
                .into_iter()
                .map(|r| RunResponse {
                    id: r.id,
                    plan_id: r.plan_id,
                    status: r.status,
                    total: r.total,
                    pass_rate: r.pass_rate,
                    execute_rate: r.execute_rate,
                    triggered_at: r.triggered_at,
                })
                .collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}
