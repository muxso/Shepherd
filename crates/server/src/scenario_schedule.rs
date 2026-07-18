//! Scenario cron schedules: batch upsert/toggle from the list page plus a
//! poll-based runner. Every tick claims due schedules with a CAS on
//! last_run_at so concurrent server instances never double-fire.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{FromRef, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use chrono::{Local, TimeZone};
use croner::Cron;
use migrate::PgPool;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use crate::scenario_run::ScenarioRunner;

#[derive(Clone)]
struct SchedState {
    pool: PgPool,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<SchedState> for Arc<dyn SessionStore> {
    fn from_ref(s: &SchedState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(pool: PgPool, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/api/scenario/schedule", get(list_schedules))
        .route("/api/scenario/schedule/batch", put(batch_upsert))
        .with_state(SchedState { pool, sessions })
}

fn parse_cron(expr: &str) -> Result<Cron, ()> {
    Cron::new(expr).with_seconds_optional().parse().map_err(|_| ())
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ScheduleDto {
    scenario_id: String,
    cron: String,
    env_mode: String,
    env_id: Option<String>,
    pool_id: Option<String>,
    enabled: bool,
    last_run_at: String,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    project_id: String,
}

#[utoipa::path(get, path = "/api/scenario/schedule", tag = "api-scenario", params(ListQuery), responses((status = 200, body = [ScheduleDto]), (status = 403)), security(("bearer" = [])))]
async fn list_schedules(
    user: AuthUser,
    State(st): State<SchedState>,
    Query(q): Query<ListQuery>,
) -> Response {
    if !user.can("API_SCENARIO", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let rows = sqlx::query(
        "SELECT scenario_id, cron, env_mode, env_id, pool_id, enabled, last_run_at::text AS last_run_at \
         FROM ms_api_scenario_schedule WHERE project_id = $1",
    )
    .bind(&q.project_id)
    .fetch_all(&st.pool)
    .await;
    match rows {
        Ok(rows) => {
            let items: Vec<ScheduleDto> = rows
                .iter()
                .map(|r| ScheduleDto {
                    scenario_id: r.try_get("scenario_id").unwrap_or_default(),
                    cron: r.try_get("cron").unwrap_or_default(),
                    env_mode: r.try_get("env_mode").unwrap_or_default(),
                    env_id: r.try_get("env_id").ok().flatten(),
                    pool_id: r.try_get("pool_id").ok().flatten(),
                    enabled: r.try_get("enabled").unwrap_or(false),
                    last_run_at: r.try_get("last_run_at").unwrap_or_default(),
                })
                .collect();
            Json(items).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct BatchBody {
    scenario_ids: Vec<String>,
    #[serde(default)]
    project_id: String,
    /// Present = create/update the schedule definition; absent = toggle-only.
    #[serde(default)]
    cron: Option<String>,
    #[serde(default)]
    env_mode: Option<String>,
    #[serde(default)]
    env_id: Option<String>,
    #[serde(default)]
    pool_id: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct BatchResult {
    updated: u64,
}

#[utoipa::path(put, path = "/api/scenario/schedule/batch", tag = "api-scenario", request_body = BatchBody, responses((status = 200, body = BatchResult), (status = 400), (status = 403)), security(("bearer" = [])))]
async fn batch_upsert(
    user: AuthUser,
    State(st): State<SchedState>,
    Json(b): Json<BatchBody>,
) -> Response {
    if !user.can("API_SCENARIO", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    if b.scenario_ids.is_empty() {
        return (StatusCode::BAD_REQUEST, "scenarioIds required").into_response();
    }
    let mut updated = 0u64;
    if let Some(cron) = b.cron.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if parse_cron(cron).is_err() {
            return (StatusCode::BAD_REQUEST, "invalid cron expression").into_response();
        }
        let env_mode = b.env_mode.as_deref().unwrap_or("DEFAULT");
        let env_id = b.env_id.as_deref().filter(|s| !s.trim().is_empty());
        let pool_id = b.pool_id.as_deref().filter(|s| !s.trim().is_empty());
        for sid in &b.scenario_ids {
            let res = sqlx::query(
                "INSERT INTO ms_api_scenario_schedule (scenario_id, project_id, cron, env_mode, env_id, pool_id, enabled, created_by) \
                 VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, true), $8) \
                 ON CONFLICT (scenario_id) DO UPDATE SET cron = $3, env_mode = $4, env_id = $5, pool_id = $6, \
                     enabled = COALESCE($7, ms_api_scenario_schedule.enabled), updated_at = now()",
            )
            .bind(sid)
            .bind(&b.project_id)
            .bind(cron)
            .bind(env_mode)
            .bind(env_id)
            .bind(pool_id)
            .bind(b.enabled)
            .bind(&user.user_id)
            .execute(&st.pool)
            .await;
            if let Ok(r) = res {
                updated += r.rows_affected();
            }
        }
    } else if let Some(enabled) = b.enabled {
        let res = sqlx::query(
            "UPDATE ms_api_scenario_schedule SET enabled = $2, updated_at = now() \
             WHERE scenario_id = ANY($1)",
        )
        .bind(&b.scenario_ids)
        .bind(enabled)
        .execute(&st.pool)
        .await;
        if let Ok(r) = res {
            updated = r.rows_affected();
        }
    } else {
        return (StatusCode::BAD_REQUEST, "cron or enabled required").into_response();
    }
    Json(BatchResult { updated }).into_response()
}

/// Poll loop: every 30s fire schedules whose next cron occurrence (local time,
/// computed from last_run_at) has passed. The CAS update claims the row.
pub fn spawn(pool: PgPool, runner: ScenarioRunner) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let rows = match sqlx::query(
                "SELECT d.scenario_id, d.project_id, d.cron, d.env_mode, d.env_id, \
                        EXTRACT(EPOCH FROM d.last_run_at)::bigint AS last_epoch, \
                        s.meta->>'envId' AS scenario_env \
                 FROM ms_api_scenario_schedule d \
                 LEFT JOIN ms_api_scenario s ON s.id = d.scenario_id AND NOT s.deleted \
                 WHERE d.enabled",
            )
            .fetch_all(&pool)
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("scenario schedule query failed: {e}");
                    continue;
                }
            };
            for r in rows {
                let sid: String = r.try_get("scenario_id").unwrap_or_default();
                let project: String = r.try_get("project_id").unwrap_or_default();
                let expr: String = r.try_get("cron").unwrap_or_default();
                let env_mode: String = r.try_get("env_mode").unwrap_or_default();
                let env_id: Option<String> = r.try_get("env_id").ok().flatten();
                let last_epoch: i64 = r.try_get("last_epoch").unwrap_or(0);
                let Ok(cron) = parse_cron(&expr) else { continue };
                let Some(last) = Local.timestamp_opt(last_epoch, 0).single() else { continue };
                let due = match cron.find_next_occurrence(&last, false) {
                    Ok(next) => next <= Local::now(),
                    Err(_) => false,
                };
                if !due {
                    continue;
                }
                // Claim: only the instance that flips last_run_at runs the scenario.
                let claimed = sqlx::query(
                    "UPDATE ms_api_scenario_schedule SET last_run_at = now() \
                     WHERE scenario_id = $1 AND EXTRACT(EPOCH FROM last_run_at)::bigint = $2",
                )
                .bind(&sid)
                .bind(last_epoch)
                .execute(&pool)
                .await
                .map(|res| res.rows_affected() > 0)
                .unwrap_or(false);
                if !claimed {
                    continue;
                }
                // DEFAULT mode uses the scenario's own configured environment.
                let scenario_env: Option<String> = r.try_get("scenario_env").ok().flatten();
                let env =
                    if env_mode == "NEW" { env_id.as_deref() } else { scenario_env.as_deref() };
                match runner.run(&sid, &project, env, false).await {
                    Ok(o) => tracing::info!(scenario = %sid, status = %o.status, report = %o.report_id, "scheduled scenario executed"),
                    Err(_) => tracing::warn!(scenario = %sid, "scheduled scenario execute failed"),
                }
            }
        }
    });
}

#[derive(OpenApi)]
#[openapi(
    paths(list_schedules, batch_upsert),
    components(schemas(ScheduleDto, BatchBody, BatchResult)),
    tags((name = "api-scenario", description = "场景"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
