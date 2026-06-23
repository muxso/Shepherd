//! 项目级接口用例执行汇总:`GET /api/case-exec-summary?projectId=X`。
//!
//! 据 `ms_api_case_result`(执行明细)JOIN `ms_api_case`(接口用例,带 project_id)聚合,
//! 给首页「接口用例数」面板用:执行次数 / 通过数 / 已执行用例数。用例总数前端已有。
//! 只读端点,需登录。

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
    /// 执行明细总条数。
    executions: i64,
    /// 其中通过(outcome=SUCCESS)条数。
    passed: i64,
    /// 至少执行过一次的不同接口用例数。
    executed_cases: i64,
}

async fn case_exec_summary(_user: AuthUser, State(st): State<St>, Query(q): Query<SummaryQuery>) -> Response {
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
