use axum::{
    extract::{FromRef, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use utoipa::ToSchema;
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct St {
    pool: PgPool,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<St> for Arc<dyn SessionStore> {
    fn from_ref(s: &St) -> Self {
        s.sessions.clone()
    }
}

pub fn router(pool: PgPool, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/api/case-exec-summary", get(case_exec_summary))
        .route("/api/exec-trend", get(exec_trend))
        .with_state(St { pool, sessions })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryQuery {
    project_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CaseExecSummary {
    executions: i64,
    passed: i64,
    executed_cases: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrendQuery {
    project_id: String,
    #[serde(default = "default_days")]
    days: i32,
}
fn default_days() -> i32 {
    7
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct TrendPoint {
    date: String,
    executions: i64,
    passed: i64,
}

async fn exec_trend(
    _user: AuthUser,
    State(st): State<St>,
    Query(q): Query<TrendQuery>,
) -> Response {
    let days = q.days.clamp(1, 90);
    let sql = "SELECT to_char(r.executed_at::date, 'YYYY-MM-DD') AS d, \
                      count(*) AS executions, \
                      count(*) FILTER (WHERE r.outcome = 'SUCCESS') AS passed \
               FROM ms_api_case_result r \
               JOIN ms_api_case c ON c.id = r.case_id \
               WHERE c.project_id = $1 AND r.executed_at >= now() - make_interval(days => $2) \
               GROUP BY r.executed_at::date \
               ORDER BY r.executed_at::date";
    match sqlx::query(sql).bind(&q.project_id).bind(days).fetch_all(&st.pool).await {
        Ok(rows) => {
            let points: Vec<TrendPoint> = rows
                .iter()
                .map(|r| TrendPoint {
                    date: r.try_get("d").unwrap_or_default(),
                    executions: r.try_get("executions").unwrap_or(0),
                    passed: r.try_get("passed").unwrap_or(0),
                })
                .collect();
            Json(points).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

async fn case_exec_summary(
    _user: AuthUser,
    State(st): State<St>,
    Query(q): Query<SummaryQuery>,
) -> Response {
    let sql = "SELECT count(*) AS executions, \
                      count(*) FILTER (WHERE r.outcome = 'SUCCESS') AS passed, \
                      count(DISTINCT r.case_id) AS executed_cases \
               FROM ms_api_case_result r \
               JOIN ms_api_case c ON c.id = r.case_id \
               WHERE c.project_id = $1";
    match sqlx::query(sql).bind(&q.project_id).fetch_one(&st.pool).await {
        Ok(row) => Json(CaseExecSummary {
            executions: row.try_get("executions").unwrap_or(0),
            passed: row.try_get("passed").unwrap_or(0),
            executed_cases: row.try_get("executed_cases").unwrap_or(0),
        })
        .into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}
