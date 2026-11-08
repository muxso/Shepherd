use super::*;
use serde_json::Value;

pub(super) fn routes() -> Router<RagState> {
    Router::new()
        .route("/rag/onboarding", post(publish).get(list))
        .route("/rag/onboarding/{id}", axum::routing::get(detail))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Publish {
    project_id: String,
    document_id: Option<String>,
    title: String,
    text: String,
    source_url: String,
    visibility_groups: Vec<String>,
    expected_version: u32,
}
async fn publish(user: AuthUser, State(st): State<RagState>, Json(b): Json<Publish>) -> Response {
    if let Err(e) = authorize_project(&user, &st.pool, &b.project_id, "ADD").await {
        return e;
    }
    if b.title.trim().is_empty()
        || b.title.chars().count() > 200
        || b.text.trim().is_empty()
        || b.text.len() > 200_000
        || (!b.source_url.is_empty() && crate::references::safe_url(&b.source_url).is_none())
    {
        return (
            StatusCode::BAD_REQUEST,
            "title, maintained content and valid source URL are required",
        )
            .into_response();
    }
    let id = b.document_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut tx = match st.pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,719))")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let existing = sqlx::query_as::<_, (String, Option<String>, String)>(
        "SELECT project_id,owner_id,source_type FROM ms_rag_document WHERE id=$1",
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await;
    let original_owner =
        existing.as_ref().ok().and_then(|row| row.as_ref()).and_then(|(_, owner, _)| owner.clone());
    match existing {
        Ok(Some((project, owner, kind)))
            if project != b.project_id
                || kind != "onboarding"
                || (!is_rag_admin(&user) && owner.as_deref() != Some(&user.user_id)) =>
        {
            return StatusCode::FORBIDDEN.into_response()
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        _ => {}
    }
    let current = match sqlx::query_scalar::<_, i32>(
        "SELECT COALESCE(max(version),0) FROM ms_rag_onboarding_revision WHERE document_id=$1",
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if current as u32 != b.expected_version {
        return (StatusCode::CONFLICT, "document changed; reload before publishing")
            .into_response();
    }
    let version = current + 1;
    let ts = now_ms();
    let doc = RagDocument {
        id: id.clone(),
        project_id: b.project_id.clone(),
        source_type: "onboarding".into(),
        source_id: Some(json!({"version":version,"url":b.source_url,"updatedAt":ts}).to_string()),
        title: b.title.clone(),
        owner_id: original_owner.or_else(|| Some(user.user_id.clone())),
        visibility_groups: b.visibility_groups.clone(),
        created_at: ts,
        updated_at: ts,
    };
    let output = match ingest(st.store.as_ref(), st.embedder.as_ref(), doc, &b.text).await {
        Ok(v) => v,
        Err(e) => return rag_error_response(e),
    };
    if sqlx::query("INSERT INTO ms_rag_onboarding_revision(document_id,version,project_id,title,content,source_url,visibility_groups,updated_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(&id).bind(version).bind(&b.project_id).bind(b.title).bind(b.text).bind(b.source_url).bind(b.visibility_groups).bind(user.user_id).execute(&mut *tx).await.is_err(){return StatusCode::INTERNAL_SERVER_ERROR.into_response()}
    if tx.commit().await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Json(json!({"documentId":id,"version":version,"chunks":output.chunks,"embedded":output.embedded})).into_response()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct List {
    project_id: String,
}
async fn list(user: AuthUser, State(st): State<RagState>, Query(q): Query<List>) -> Response {
    if let Err(e) = authorize_project(&user, &st.pool, &q.project_id, "READ").await {
        return e;
    }
    let a = load_audience(&user, &st.pool).await;
    let result=sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('documentId',d.id,'title',d.title,'updatedAt',d.updated_at,'ownerId',d.owner_id,'visibilityGroups',d.visibility_groups,'version',(SELECT max(version) FROM ms_rag_onboarding_revision r WHERE r.document_id=d.id)) FROM ms_rag_document d WHERE d.project_id=$1 AND d.source_type='onboarding' AND ($2 OR d.owner_id=$3 OR d.visibility_groups && $4) ORDER BY d.updated_at DESC LIMIT 500").bind(&q.project_id).bind(a.is_admin).bind(a.user_id).bind(a.visible_group_ids).fetch_all(&st.pool).await;
    match result {
        Ok(items) => Json(json!({"items":items})).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
async fn detail(user: AuthUser, State(st): State<RagState>, Path(id): Path<String>) -> Response {
    let doc=sqlx::query_as::<_,(String,Option<String>,Vec<String>)>("SELECT project_id,owner_id,visibility_groups FROM ms_rag_document WHERE id=$1 AND source_type='onboarding'").bind(&id).fetch_optional(&st.pool).await;
    let (project, owner, groups) = match doc {
        Ok(Some(v)) => v,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    if let Err(e) = authorize_project(&user, &st.pool, &project, "READ").await {
        return e;
    }
    let a = load_audience(&user, &st.pool).await;
    if !a.is_admin
        && owner.as_deref() != Some(&user.user_id)
        && !groups.iter().any(|g| a.visible_group_ids.contains(g))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let revision=sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('documentId',document_id,'projectId',project_id,'title',title,'text',content,'version',version,'sourceUrl',source_url,'visibilityGroups',visibility_groups,'updatedAt',updated_at,'updatedBy',updated_by) FROM ms_rag_onboarding_revision WHERE document_id=$1 ORDER BY version DESC LIMIT 1").bind(&id).fetch_optional(&st.pool).await;
    match revision {
        Ok(Some(v)) => Json(v).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{body, fixture};
    use super::*;
    #[tokio::test]
    #[ignore = "requires isolated SHEPHERD_RAG_TEST_DATABASE_URL"]
    async fn maintained_knowledge_has_versions_and_live_visibility() {
        let (st, user, project, _) = fixture().await;
        let request = |id: Option<String>, version: u32, text: &str| Publish {
            project_id: project.clone(),
            document_id: id,
            title: "New employee guide".into(),
            text: text.into(),
            source_url: "https://example.com/onboarding".into(),
            visibility_groups: vec![],
            expected_version: version,
        };
        let created = body(
            publish(
                user.clone(),
                State(st.clone()),
                Json(request(None, 0, "# New employee guide\nAsk the project owner for access.")),
            )
            .await,
        )
        .await;
        let id = created["documentId"].as_str().unwrap().to_owned();
        assert_eq!(created["version"], 1);
        let revised = body(
            publish(
                user.clone(),
                State(st.clone()),
                Json(request(
                    Some(id.clone()),
                    1,
                    "# New employee guide\nUse the access request page.",
                )),
            )
            .await,
        )
        .await;
        assert_eq!(revised["version"], 2);
        assert_eq!(
            publish(
                user.clone(),
                State(st.clone()),
                Json(request(Some(id.clone()), 1, "stale content"))
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );
        let latest = body(detail(user.clone(), State(st.clone()), Path(id.clone())).await).await;
        assert!(latest["text"].as_str().unwrap().contains("access request page"));
        let mut outsider = user.clone();
        outsider.user_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO ms_project_member(project_id,user_id) VALUES($1,$2)")
            .bind(&project)
            .bind(&outsider.user_id)
            .execute(&st.pool)
            .await
            .unwrap();
        assert_eq!(
            detail(outsider.clone(), State(st.clone()), Path(id)).await.status(),
            StatusCode::NOT_FOUND
        );
        let audience = load_audience(&outsider, &st.pool).await;
        assert!(st
            .store
            .scoped_search(&project, &[1.0], "入职", "onboarding", 8, &audience)
            .await
            .unwrap()
            .unwrap()
            .is_empty());
        let audience = load_audience(&user, &st.pool).await;
        let hits = st
            .store
            .scoped_search(&project, &[1.0], "入职", "onboarding", 8, &audience)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].content.contains("access request page"));
        let revision_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ms_rag_onboarding_revision WHERE document_id=$1",
        )
        .bind(&hits[0].document_id)
        .fetch_one(&st.pool)
        .await
        .unwrap();
        assert_eq!(revision_count, 2);
    }
}
