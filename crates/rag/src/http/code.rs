use super::{authorize_project, load_audience, RagState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use webauth::AuthUser;
pub(super) fn routes() -> Router<RagState> {
    Router::new()
        .route("/rag/code/checkpoint", post(checkpoint))
        .route("/rag/code/activity", post(activity))
        .route("/rag/code/repositories", get(repositories))
        .route("/rag/code/repositories/{id}/inventory", post(inventory))
}
fn err(message: &str, code: StatusCode) -> Response {
    (code, Json(json!({"error":message}))).into_response()
}
fn valid_sha(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|c| c.is_ascii_hexdigit())
}
fn valid_path(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 2000
        && !s.starts_with('/')
        && !s.contains('\\')
        && !s.split('/').any(|p| p == "..")
        && !s.chars().any(char::is_control)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct File {
    path: String,
    document_id: String,
    blob: String,
    content_commit: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Checkpoint {
    project_id: String,
    repository_id: String,
    name: String,
    url: String,
    branch: String,
    commit_sha: String,
    files: Vec<File>,
}
async fn checkpoint(u: AuthUser, State(s): State<RagState>, Json(b): Json<Checkpoint>) -> Response {
    if let Err(r) = authorize_project(&u, &s.pool, &b.project_id, "ADD").await {
        return r;
    }
    if !valid_sha(&b.commit_sha)
        || b.repository_id.trim().is_empty()
        || b.repository_id.len() > 255
        || b.name.trim().is_empty()
        || b.name.len() > 500
        || b.branch.trim().is_empty()
        || b.branch.len() > 255
        || (!b.url.is_empty() && !b.url.starts_with("https://"))
        || b.files.len() > 50000
    {
        return err("invalid repository checkpoint", StatusCode::BAD_REQUEST);
    }
    let ids: Vec<_> = b.files.iter().map(|f| f.document_id.clone()).collect();
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
        || b.files.iter().map(|f| &f.path).collect::<BTreeSet<_>>().len() != ids.len()
        || b.files
            .iter()
            .any(|f| !valid_path(&f.path) || !valid_sha(&f.blob) || !valid_sha(&f.content_commit))
    {
        return err(
            "checkpoint paths, document IDs and blob IDs must be valid and unique",
            StatusCode::BAD_REQUEST,
        );
    }
    let a = load_audience(&u, &s.pool).await;
    let rows=sqlx::query_as::<_,(String,String,Option<String>,Vec<String>,i64)>("SELECT id,COALESCE(source_id,''),owner_id,visibility_groups,updated_at FROM ms_rag_document WHERE project_id=$1 AND id=ANY($2) AND source_type='code'").bind(&b.project_id).bind(&ids).fetch_all(&s.pool).await;
    let rows = match rows {
        Ok(rows) if rows.len() == ids.len() => rows,
        Ok(_) => {
            return err("checkpoint contains unavailable code documents", StatusCode::CONFLICT)
        }
        Err(_) => return err("checkpoint storage unavailable", StatusCode::INTERNAL_SERVER_ERROR),
    };
    let docs: BTreeMap<_, _> = rows
        .into_iter()
        .map(|(id, source, owner, groups, updated)| (id, (source, owner, groups, updated)))
        .collect();
    let mut common: Option<BTreeSet<String>> = None;
    let mut files = Vec::new();
    for f in &b.files {
        let Some((source, owner, groups, updated)) = docs.get(&f.document_id) else {
            return err("missing code document", StatusCode::CONFLICT);
        };
        if !a.is_admin && owner.as_deref() != Some(u.user_id.as_str()) {
            return err(
                "only the code uploader can publish this checkpoint",
                StatusCode::FORBIDDEN,
            );
        }
        let meta: Value = serde_json::from_str(source).unwrap_or(Value::Null);
        if meta["repositoryId"] != b.repository_id
            || meta["branch"] != b.branch
            || meta["path"] != f.path
            || meta["commit"] != f.content_commit
            || meta["blob"].as_str().is_some_and(|blob| blob != f.blob)
        {
            return err("checkpoint does not match code provenance", StatusCode::CONFLICT);
        }
        let g: BTreeSet<_> = groups.iter().cloned().collect();
        common = Some(common.map(|c| c.intersection(&g).cloned().collect()).unwrap_or(g));
        files.push(json!({"path":f.path,"documentId":f.document_id,"blob":f.blob,"documentUpdatedAt":updated}));
    }
    let groups: Vec<String> = common.unwrap_or_default().into_iter().collect();
    let result=async{
  let mut tx=s.pool.begin().await?;
  sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))").bind(format!("code-checkpoint:{}:{}:{}",b.project_id,b.repository_id,b.branch)).execute(&mut *tx).await?;
  for f in &b.files{
   let updated=docs.get(&f.document_id).map(|d|d.3).unwrap_or(0);
   let n=sqlx::query("UPDATE ms_rag_document SET code_blob=$3,source_id=(source_id::jsonb||jsonb_build_object('blob',$3::text))::text WHERE project_id=$1 AND id=$2 AND updated_at=$4").bind(&b.project_id).bind(&f.document_id).bind(&f.blob).bind(updated).execute(&mut *tx).await?.rows_affected();
   if n!=1{return Err(sqlx::Error::RowNotFound);}
  }
  sqlx::query("INSERT INTO ms_rag_repository_checkpoint(project_id,repository_id,branch,commit_sha,name,url,files,owner_id,visibility_groups) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(project_id,repository_id,branch) DO UPDATE SET commit_sha=EXCLUDED.commit_sha,name=EXCLUDED.name,url=EXCLUDED.url,files=EXCLUDED.files,owner_id=EXCLUDED.owner_id,visibility_groups=EXCLUDED.visibility_groups,updated_at=now()").bind(&b.project_id).bind(&b.repository_id).bind(&b.branch).bind(&b.commit_sha).bind(&b.name).bind(&b.url).bind(json!(files)).bind(&u.user_id).bind(groups).execute(&mut *tx).await?;
  tx.commit().await
 }.await;
    match result {
        Ok(()) => Json(json!({"published":true,"files":b.files.len(),"commitSha":b.commit_sha}))
            .into_response(),
        Err(_) => err(
            "code changed while publishing checkpoint; retry after indexing completes",
            StatusCode::CONFLICT,
        ),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectQuery {
    project_id: String,
}
async fn repositories(
    u: AuthUser,
    State(s): State<RagState>,
    Query(q): Query<ProjectQuery>,
) -> Response {
    if let Err(r) = authorize_project(&u, &s.pool, &q.project_id, "READ").await {
        return r;
    }
    let a = load_audience(&u, &s.pool).await;
    let rows=sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('id',c.repository_id,'name',c.name,'url',c.url,'branch',c.branch,'commitSha',c.commit_sha,'files',jsonb_array_length(c.files),'updatedAt',c.updated_at) FROM ms_rag_repository_checkpoint c WHERE c.project_id=$1 AND ($2 OR c.owner_id=$3 OR c.visibility_groups&&$4::text[]) AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(c.files) f LEFT JOIN ms_rag_document d ON d.id=f->>'documentId' AND d.project_id=c.project_id WHERE d.id IS NULL OR NOT($2 OR d.owner_id=$3 OR d.visibility_groups&&$4::text[])) ORDER BY c.name,c.branch").bind(&q.project_id).bind(a.is_admin).bind(&a.user_id).bind(&a.visible_group_ids).fetch_all(&s.pool).await;
    match rows {
        Ok(rows) => Json(json!({"items":rows})).into_response(),
        Err(_) => err("repository catalog unavailable", StatusCode::INTERNAL_SERVER_ERROR),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InventoryRequest {
    project_id: String,
    service_id: String,
    branch: String,
}
async fn inventory(
    u: AuthUser,
    State(s): State<RagState>,
    Path(id): Path<String>,
    Json(b): Json<InventoryRequest>,
) -> Response {
    if let Err(r) = authorize_project(&u, &s.pool, &b.project_id, "READ").await {
        return r;
    }
    if !u.can("FUNCTIONAL_CASE", "ADD") {
        return err("permission denied", StatusCode::FORBIDDEN);
    }
    let a = load_audience(&u, &s.pool).await;
    let checkpoint=sqlx::query_scalar::<_,Value>("SELECT to_jsonb(c) FROM ms_rag_repository_checkpoint c WHERE project_id=$1 AND repository_id=$2 AND branch=$3 AND ($4 OR owner_id=$5 OR visibility_groups&&$6::text[])").bind(&b.project_id).bind(&id).bind(&b.branch).bind(a.is_admin).bind(&a.user_id).bind(&a.visible_group_ids).fetch_optional(&s.pool).await;
    let cp = match checkpoint {
        Ok(Some(cp)) => cp,
        Ok(None) => return err("no readable repository checkpoint", StatusCode::NOT_FOUND),
        Err(_) => return err("checkpoint unavailable", StatusCode::INTERNAL_SERVER_ERROR),
    };
    let bound=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM ms_test_service s,jsonb_array_elements(s.repositories) r WHERE s.project_id=$1 AND s.id=$2 AND NOT s.deleted AND r->>'id'=$3 AND r->>'branch'=$4 AND r->>'commitSha'=$5)").bind(&b.project_id).bind(&b.service_id).bind(&id).bind(&b.branch).bind(cp["commit_sha"].as_str().unwrap_or("")).fetch_one(&s.pool).await;
    if !matches!(bound, Ok(true)) {
        return err(
            "service repository baseline must match the indexed checkpoint",
            StatusCode::CONFLICT,
        );
    }
    let manifest: Vec<Value> = serde_json::from_value(cp["files"].clone()).unwrap_or_default();
    let doc_ids: Vec<String> =
        manifest.iter().filter_map(|f| f["documentId"].as_str().map(str::to_owned)).collect();
    let docs=sqlx::query_as::<_,(String,String,i64)>("SELECT id,COALESCE(source_id,''),updated_at FROM ms_rag_document d WHERE project_id=$1 AND id=ANY($2) AND ($3 OR owner_id=$4 OR visibility_groups&&$5::text[])").bind(&b.project_id).bind(&doc_ids).bind(a.is_admin).bind(&a.user_id).bind(&a.visible_group_ids).fetch_all(&s.pool).await;
    let docs = match docs {
        Ok(d) if d.len() == doc_ids.len() => d,
        Ok(_) => return err("repository files are not all readable", StatusCode::FORBIDDEN),
        Err(_) => return err("code documents unavailable", StatusCode::INTERNAL_SERVER_ERROR),
    };
    let current: BTreeMap<_, _> =
        docs.into_iter().map(|(id, source, updated)| (id, (source, updated))).collect();
    for f in &manifest {
        let Some((source, updated)) = f["documentId"].as_str().and_then(|id| current.get(id))
        else {
            return err("checkpoint document missing", StatusCode::CONFLICT);
        };
        let meta: Value = serde_json::from_str(source).unwrap_or(Value::Null);
        if *updated != f["documentUpdatedAt"].as_i64().unwrap_or(-1) || meta["blob"] != f["blob"] {
            return err(
                "repository index changed; publish a fresh checkpoint",
                StatusCode::CONFLICT,
            );
        }
    }
    let rows=sqlx::query_as::<_,(String,String)>("SELECT document_id,content FROM ms_rag_chunk WHERE project_id=$1 AND document_id=ANY($2) ORDER BY document_id,chunk_index").bind(&b.project_id).bind(&doc_ids).fetch_all(&s.pool).await;
    let rows = match rows {
        Ok(rows) => rows,
        Err(_) => return err("code chunks unavailable", StatusCode::INTERNAL_SERVER_ERROR),
    };
    let bytes: usize = rows.iter().map(|r| r.1.len()).sum();
    if bytes > 64 * 1024 * 1024 {
        return err(
            "repository code exceeds one scan budget; narrow the service repository scope",
            StatusCode::PAYLOAD_TOO_LARGE,
        );
    }
    let mut chunks: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (doc, text) in rows {
        chunks.entry(doc).or_default().push(text);
    }
    let mut files = Vec::new();
    let mut extra_limits = Vec::new();
    let mut blobs = serde_json::Map::new();
    for f in &manifest {
        let (Some(doc), Some(path)) = (f["documentId"].as_str(), f["path"].as_str()) else {
            continue;
        };
        match crate::code_inventory::reconstruct(chunks.get(doc).map(Vec::as_slice).unwrap_or(&[]))
        {
            Ok(lines) => files.push((path.to_string(), doc.to_string(), lines)),
            Err(reason) => extra_limits.push(format!("{path}: {reason}")),
        };
        blobs.insert(path.into(), f["blob"].clone());
    }
    let scanned = crate::code_inventory::scan(&files);
    let mut limits = scanned.limitations;
    limits.extend(extra_limits);
    limits.push(
        "扫描范围为已成功入库的允许文件；凭据脱敏、未索引文件和运行期动态依赖不在静态证明范围内"
            .into(),
    );
    let result=sqlx::query_scalar::<_,Value>("INSERT INTO ms_test_inventory(project_id,service_id,repository_id,branch,commit_sha,state,endpoints,limitations,scanned_paths,created_by,source_document_ids,file_blobs) VALUES($1,$2,$3,$4,$5,'PARTIAL',$6,$7,$8,$9,$10,$11) RETURNING to_jsonb(ms_test_inventory)").bind(&b.project_id).bind(&b.service_id).bind(&id).bind(&b.branch).bind(cp["commit_sha"].as_str().unwrap_or("")).bind(json!(scanned.endpoints)).bind(json!(limits)).bind(json!(scanned.scanned_paths)).bind(&u.user_id).bind(&doc_ids).bind(Value::Object(blobs)).fetch_one(&s.pool).await;
    match result {
        Ok(v) => Json(v).into_response(),
        Err(_) => err("inventory could not be saved", StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActivityUpload {
    project_id: String,
    repository_id: String,
    commits: Vec<Value>,
}
async fn activity(
    u: AuthUser,
    State(s): State<RagState>,
    Json(b): Json<ActivityUpload>,
) -> Response {
    if let Err(r) = authorize_project(&u, &s.pool, &b.project_id, "ADD").await {
        return r;
    }
    if b.commits.len() > 500 {
        return err("at most 500 commit records per batch", StatusCode::BAD_REQUEST);
    }
    let a = load_audience(&u, &s.pool).await;
    let groups=sqlx::query_scalar::<_,Vec<String>>("SELECT visibility_groups FROM ms_rag_repository_checkpoint WHERE project_id=$1 AND repository_id=$2 AND ($3 OR owner_id=$4) ORDER BY updated_at DESC LIMIT 1").bind(&b.project_id).bind(&b.repository_id).bind(a.is_admin).bind(&u.user_id).fetch_optional(&s.pool).await;
    let groups = match groups {
        Ok(Some(g)) => g,
        _ => {
            return err(
                "publish an owned repository checkpoint before commit evidence",
                StatusCode::FORBIDDEN,
            )
        }
    };
    for c in &b.commits {
        if c["sha"].as_str().is_none_or(|v| !valid_sha(v))
            || c["authoredAt"].as_str().is_none_or(|v| v.len() > 100)
            || !c["files"].is_array()
            || c["message"].as_str().is_none_or(|v| v.len() > 100000)
        {
            return err("invalid commit evidence", StatusCode::BAD_REQUEST);
        }
        if c["files"].as_array().is_none_or(|files| {
            files.len() > 10000 || files.iter().any(|f| f.as_str().is_none_or(|v| !valid_path(v)))
        }) {
            return err("invalid changed file paths", StatusCode::BAD_REQUEST);
        }
    }
    let result=async{let mut tx=s.pool.begin().await?;for c in &b.commits{
        sqlx::query("INSERT INTO ms_rag_git_commit(project_id,repository_id,sha,data,owner_id,visibility_groups) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(project_id,repository_id,sha) DO UPDATE SET data=EXCLUDED.data,visibility_groups=EXCLUDED.visibility_groups,updated_at=now() WHERE ms_rag_git_commit.owner_id=$5 OR $7").bind(&b.project_id).bind(&b.repository_id).bind(c["sha"].as_str().unwrap_or("")).bind(c).bind(&u.user_id).bind(&groups).bind(a.is_admin).execute(&mut *tx).await?;
    }tx.commit().await}.await;
    match result {
        Ok(()) => Json(json!({"stored":b.commits.len()})).into_response(),
        Err(_) => err("commit evidence could not be saved", StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{body, fixture};
    use super::*;
    #[tokio::test]
    #[ignore = "requires isolated SHEPHERD_RAG_TEST_DATABASE_URL"]
    async fn checkpoints_verify_file_identity_and_scans_keep_source_provenance() {
        let (s, u, p, _) = fixture().await;
        let sha = "a".repeat(40);
        let blob = "b".repeat(40);
        let doc = uuid::Uuid::new_v4().to_string();
        let repo = "test-repo";
        let source=json!({"repositoryId":repo,"branch":"main","path":"src/api.py","commit":sha,"blob":blob}).to_string();
        sqlx::query("INSERT INTO ms_rag_document(id,project_id,source_type,source_id,title,owner_id,created_at,updated_at,code_repository_id,code_branch,code_path,code_commit,code_blob) VALUES($1,$2,'code',$3,'Orders',$4,1,1,$5,'main','src/api.py',$6,$7)").bind(&doc).bind(&p).bind(&source).bind(&u.user_id).bind(repo).bind(&sha).bind(&blob).execute(&s.pool).await.expect("code doc");
        sqlx::query("INSERT INTO ms_rag_chunk(id,document_id,project_id,chunk_index,heading,content,embedding,created_at) VALUES(gen_random_uuid()::text,$1,$2,0,'route','L1: @app.get(\"/orders\")\nL2: def orders():\nL3:     return []',ARRAY[]::real[],1)").bind(&doc).bind(&p).execute(&s.pool).await.expect("code");
        let make = |path: &str| Checkpoint {
            project_id: p.clone(),
            repository_id: repo.into(),
            name: "Orders".into(),
            url: "https://example.test/orders".into(),
            branch: "main".into(),
            commit_sha: sha.clone(),
            files: vec![File {
                path: path.into(),
                document_id: doc.clone(),
                blob: blob.clone(),
                content_commit: sha.clone(),
            }],
        };
        assert_eq!(
            checkpoint(u.clone(), State(s.clone()), Json(make("wrong.py"))).await.status(),
            StatusCode::CONFLICT
        );
        body(checkpoint(u.clone(), State(s.clone()), Json(make("src/api.py"))).await).await;
        let repos = body(
            repositories(
                u.clone(),
                State(s.clone()),
                Query(ProjectQuery { project_id: p.clone() }),
            )
            .await,
        )
        .await;
        assert_eq!(repos["items"].as_array().expect("repos").len(), 1);
        let service:String=sqlx::query_scalar("INSERT INTO ms_test_service(project_id,name,repositories) VALUES($1,'Orders',$2) RETURNING id").bind(&p).bind(json!([{"id":repo,"name":"Orders","branch":"main","commitSha":sha}])).fetch_one(&s.pool).await.expect("service");
        let result = body(
            inventory(
                u.clone(),
                State(s.clone()),
                Path(repo.into()),
                Json(InventoryRequest {
                    project_id: p.clone(),
                    service_id: service,
                    branch: "main".into(),
                }),
            )
            .await,
        )
        .await;
        assert_eq!(result["state"], "PARTIAL");
        assert_eq!(result["endpoints"][0]["path"], "/orders");
        assert_eq!(result["source_document_ids"], json!([doc]));
        sqlx::query("UPDATE ms_rag_document SET owner_id='another' WHERE id=$1")
            .bind(doc)
            .execute(&s.pool)
            .await
            .expect("revoke");
        let repos =
            body(repositories(u, State(s), Query(ProjectQuery { project_id: p })).await).await;
        assert!(repos["items"].as_array().expect("repos").is_empty());
    }
}
