use std::collections::HashMap;
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
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use uuid::Uuid;
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
    /// Registered cron job ids per plan; jobs live only in memory, so
    /// re-save/delete must unhook them here or they fire until restart.
    jobs: Arc<Mutex<HashMap<String, Uuid>>>,
    sessions: Arc<dyn SessionStore>,
}

async fn unregister_job(sched: &JobScheduler, jobs: &Mutex<HashMap<String, Uuid>>, plan_id: &str) {
    if let Some(uuid) = jobs.lock().await.remove(plan_id) {
        if let Err(e) = sched.remove(&uuid).await {
            tracing::warn!(plan = %plan_id, "remove cron job failed: {e}");
        }
    }
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
) -> Option<Uuid> {
    let uc = run_uc.clone();
    let runner = runner.clone();
    let pid = plan_id.to_string();
    let job = Job::new_async(cron, move |_uuid, _l| {
        let uc = uc.clone();
        let runner = runner.clone();
        let pid = pid.clone();
        Box::pin(async move {
            match runner.run(&pid, None, None).await {
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
            let uuid = j.guid();
            match sched.add(j).await {
                Ok(_) => Some(uuid),
                Err(e) => {
                    tracing::warn!(plan = %plan_id, "add cron job failed: {e}");
                    None
                }
            }
        }
        Err(e) => {
            tracing::warn!(plan = %plan_id, cron, "invalid cron: {e}");
            None
        }
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
    let plan_runner = crate::plan_run::PlanRunner::new(pool.clone(), None);

    let sched = JobScheduler::new().await?;
    let mut registered = HashMap::new();
    if let Ok(schedules) = create.list_enabled().await {
        for s in schedules {
            if let Some(uuid) =
                register_job(&sched, &run_uc, &plan_runner, &s.plan_id, &s.cron).await
            {
                registered.insert(s.plan_id.clone(), uuid);
            }
        }
    }
    sched.start().await?;
    let sched = Arc::new(sched);
    let jobs = Arc::new(Mutex::new(registered));

    Ok(Router::new()
        .route("/test-plan/{id}/schedule", post(create_schedule).delete(delete_schedule))
        .route("/test-plan/{id}/runs", get(list_runs))
        .with_state(SchedState { create, run_uc, plan_runner, sched, jobs, sessions }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleBody {
    cron: String,
    /// Absent = enabled (backwards compatible).
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_enabled() -> bool {
    true
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
    match st.create.execute(&id, &b.cron, b.enabled).await {
        Ok(s) => {
            unregister_job(&st.sched, &st.jobs, &s.plan_id).await;
            if s.enabled {
                if let Some(uuid) =
                    register_job(&st.sched, &st.run_uc, &st.plan_runner, &s.plan_id, &s.cron).await
                {
                    st.jobs.lock().await.insert(s.plan_id.clone(), uuid);
                }
            }
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

async fn delete_schedule(
    user: AuthUser,
    State(st): State<SchedState>,
    Path(id): Path<String>,
) -> Response {
    // Same capability as configuring a schedule (plans have no DELETE action).
    if !user.can("TEST_PLAN", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    unregister_job(&st.sched, &st.jobs, &id).await;
    match st.create.delete(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "no schedule for plan").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
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
