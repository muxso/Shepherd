//! 项目文件管理:`/api/project-file`(列表/上传/下载/删除)。
//!
//! 内容以 base64 文本落 PG(小文件;受全局 2MB 请求体上限约束)。列表不回内容,
//! 下载单取一条回 {name, contentBase64}。上传/删除需登录,列表只读。

use axum::{
    extract::{FromRef, Path, Query, State},
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
        .route("/api/project-file", get(list_files).post(upload_file))
        .route("/api/project-file/{id}", axum::routing::delete(delete_file))
        .route("/api/project-file/{id}/module", axum::routing::put(move_file))
        .route("/api/project-file/{id}/download", get(download_file))
        .with_state(St { pool, sessions })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    project_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FileMeta {
    id: String,
    name: String,
    file_format: String,
    size_bytes: i64,
    /// 归属模块 id;None = 未规划(顶层)。
    module_id: Option<String>,
    created_by: Option<String>,
    created_at: String,
    updated_at: String,
}

async fn list_files(State(st): State<St>, Query(q): Query<ListQuery>) -> Response {
    let rows = sqlx::query(
        "SELECT id, name, file_format, size_bytes, module_id, created_by, \
                created_at::text AS created_at, updated_at::text AS updated_at \
         FROM ms_project_file WHERE project_id = $1 ORDER BY created_at DESC",
    )
    .bind(&q.project_id)
    .fetch_all(&st.pool)
    .await;
    match rows {
        Ok(rows) => {
            let items: Vec<FileMeta> = rows
                .iter()
                .map(|r| FileMeta {
                    id: r.try_get("id").unwrap_or_default(),
                    name: r.try_get("name").unwrap_or_default(),
                    file_format: r.try_get("file_format").unwrap_or_default(),
                    size_bytes: r.try_get("size_bytes").unwrap_or(0),
                    module_id: r.try_get("module_id").ok().flatten(),
                    created_by: r.try_get("created_by").ok().flatten(),
                    created_at: r.try_get("created_at").unwrap_or_default(),
                    updated_at: r.try_get("updated_at").unwrap_or_default(),
                })
                .collect();
            Json(items).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct UploadBody {
    project_id: String,
    name: String,
    #[serde(default)]
    file_format: String,
    #[serde(default)]
    size_bytes: i64,
    /// 目标模块 id;null/缺省/空 = 未规划。
    #[serde(default)]
    module_id: Option<String>,
    /// base64 文件内容(不含 data URI 前缀)。
    content_base64: String,
}

async fn upload_file(user: AuthUser, State(st): State<St>, Json(b): Json<UploadBody>) -> Response {
    if b.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "name required").into_response();
    }
    let module = b.module_id.as_deref().filter(|s| !s.is_empty());
    let row = sqlx::query(
        "INSERT INTO ms_project_file (project_id, name, file_format, size_bytes, content_b64, created_by, module_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(&b.project_id)
    .bind(b.name.trim())
    .bind(&b.file_format)
    .bind(b.size_bytes)
    .bind(&b.content_base64)
    .bind(&user.user_id)
    .bind(module)
    .fetch_one(&st.pool)
    .await;
    match row {
        Ok(r) => {
            let id: String = r.try_get("id").unwrap_or_default();
            (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct DownloadResp {
    name: String,
    content_base64: String,
}

async fn download_file(State(st): State<St>, Path(id): Path<String>) -> Response {
    let row = sqlx::query("SELECT name, content_b64 FROM ms_project_file WHERE id = $1")
        .bind(&id)
        .fetch_optional(&st.pool)
        .await;
    match row {
        Ok(Some(r)) => Json(DownloadResp {
            name: r.try_get("name").unwrap_or_default(),
            content_base64: r.try_get("content_b64").unwrap_or_default(),
        })
        .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "file not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

async fn delete_file(user: AuthUser, State(st): State<St>, Path(id): Path<String>) -> Response {
    if !user.can("PROJECT", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match sqlx::query("DELETE FROM ms_project_file WHERE id = $1").bind(&id).execute(&st.pool).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct MoveFileBody {
    /// 目标模块 id;null/缺省/空 = 移出到未规划。
    #[serde(default)]
    module_id: Option<String>,
}

async fn move_file(user: AuthUser, State(st): State<St>, Path(id): Path<String>, Json(b): Json<MoveFileBody>) -> Response {
    if !user.can("PROJECT", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let module = b.module_id.as_deref().filter(|s| !s.is_empty());
    match sqlx::query("UPDATE ms_project_file SET module_id = $2, updated_at = now() WHERE id = $1")
        .bind(&id)
        .bind(module)
        .execute(&st.pool)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}
