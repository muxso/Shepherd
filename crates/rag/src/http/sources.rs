use super::{authorize_project, load_audience, RagState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use webauth::AuthUser;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SourceQuery {
    chunk_id: Option<String>,
}

pub(super) async fn get_document(
    user: AuthUser,
    State(state): State<RagState>,
    Path(id): Path<String>,
    Query(query): Query<SourceQuery>,
) -> Response {
    type Document = (String, String, String, Option<String>, i64);
    let row: Option<Document> = match sqlx::query_as(
        "SELECT project_id,title,source_type,source_id,updated_at FROM ms_rag_document WHERE id=$1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(row) => row,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "source unavailable").into_response(),
    };
    let Some((project, title, kind, source, updated)) = row else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Err(response) = authorize_project(&user, &state.pool, &project, "READ").await {
        return response;
    }
    let audience = load_audience(&user, &state.pool).await;
    let visible: bool = match sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM ms_rag_document WHERE id=$1 AND ($2 OR owner_id=$3 OR visibility_groups && $4::text[]))"
    ).bind(&id).bind(audience.is_admin).bind(&audience.user_id).bind(&audience.visible_group_ids)
        .fetch_one(&state.pool).await {
        Ok(value) => value,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "source unavailable").into_response(),
    };
    if !visible {
        return StatusCode::NOT_FOUND.into_response();
    }
    let rows: Vec<(String, String, String, i32)> = match sqlx::query_as(
        "SELECT id,heading,content,chunk_index FROM ms_rag_chunk WHERE document_id=$1 AND ($2::text IS NULL OR id=$2) ORDER BY chunk_index,id LIMIT 101"
    ).bind(&id).bind(&query.chunk_id).fetch_all(&state.pool).await {
        Ok(rows) => rows,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "source unavailable").into_response(),
    };
    if query.chunk_id.is_some() && rows.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let truncated = rows.len() > 100;
    let chunks: Vec<_> = rows.into_iter().take(100).map(|(id, heading, content, index)| json!({"id":id,"heading":heading,"content":content,"index":index})).collect();
    let sample = crate::domain::Hit {
        chunk_id: chunks.first().and_then(|chunk| chunk["id"].as_str()).unwrap_or_default().into(),
        document_id: id.clone(),
        title: title.clone(),
        heading: String::new(),
        content: String::new(),
        score: 0.0,
    };
    let reference =
        crate::references::reference(&project, &sample, &kind, source.as_deref(), updated);
    Json(json!({"documentId":id,"projectId":project,"title":title,"sourceType":kind,"indexedAt":updated,"reference":reference,"chunks":chunks,"truncated":truncated})).into_response()
}
