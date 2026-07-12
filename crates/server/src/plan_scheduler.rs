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
    plan_runner: crate::plan_run::PlanRunner,
    sched: Arc<JobScheduler>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<SchedState> for Arc<dyn SessionStore> {
    fn from_ref(s: &SchedState) -> Self {
        s.sessions.clone()
    }
}

async fn register_job(
    sched: &JobScheduler,
    run_uc: &ScheduledRunUseCase,
    runner: &crate::plan_run::PlanRunner,
    plan_id: &str,
    cron: &str,
) {
    let uc = run_uc.clone();
    let runner = runner.clone();
    let pid = plan_id.to_string();
    let job = Job::new_async(cron, move |_uuid, _l| {
        let uc = uc.clone();
        let runner = runner.clone();
        let pid = pid.clone();
        Box::pin(async move {
            match runner.run(&pid, None).await {
                Ok(s) => {
                    tracing::info!(plan = %pid, executed = s.executed, success = s.success, failed = s.failed, "scheduled plan executed")
                }
                Err(()) => tracing::warn!(plan = %pid, "scheduled plan execute failed"),
            }
            match uc.execute(&pid).await {
                Ok(run) => {
                    tracing::info!(plan = %pid, status = %run.status, "scheduled plan run snapshot")
                }
                Err(e) => tracing::warn!(plan = %pid, "scheduled snapshot failed: {e:?}"),
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

pub async fn build(
    pool: PgPool,
    sessions: Arc<dyn SessionStore>,
) -> Result<Router, Box<dyn std::error::Error>> {
    let plan_repo = Arc::new(PgPlanRepository::new(pool.clone()));
    let store = Arc::new(PgScheduleStore::new(pool.clone()));
    let create = CreateScheduleUseCase::new(store.clone());
    let run_uc = ScheduledRunUseCase::new(PlanStatisticsUseCase::new(plan_repo), store);
    let plan_runner = crate::plan_run::PlanRunner::new(pool.clone());

    let sched = JobScheduler::new().await?;
    if let Ok(schedules) = create.list_enabled().await {
        for s in schedules {
            register_job(&sched, &run_uc, &plan_runner, &s.plan_id, &s.cron).await;
        }
    }
    sched.start().await?;
    let sched = Arc::new(sched);

    Ok(Router::new()
        .route("/test-plan/{id}/schedule", post(create_schedule))
        .route("/test-plan/{id}/runs", get(list_runs))
        .with_state(SchedState { create, run_uc, plan_runner, sched, sessions }))
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
            register_job(&st.sched, &st.run_uc, &st.plan_runner, &s.plan_id, &s.cron).await;
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
