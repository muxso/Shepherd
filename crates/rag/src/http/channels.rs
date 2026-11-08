use super::*;
use axum::{
    http::StatusCode,
    routing::{get, put},
};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) fn routes() -> Router<RagState> {
    Router::new()
        .route("/system/rag/channels", get(list).post(create))
        .route("/system/rag/channels/{id}", put(update).delete(remove))
        .route("/rag/channel/runtime", get(runtime).post(report))
        .route("/system/rag/channels/{id}/binding-code", post(binding_code))
        .route("/rag/channel/bind", post(bind))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Policy {
    name: String,
    service_user_id: String,
    enabled: bool,
    #[serde(default)]
    allow_binding: bool,
    conversations: Vec<Conversation>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Conversation {
    kind: String,
    external_id: String,
    project_id: String,
    enabled: bool,
}
fn valid_id(v: &str) -> bool {
    !v.trim().is_empty() && v.len() <= 255 && !v.chars().any(char::is_control)
}
fn valid(b: &Policy) -> bool {
    valid_id(&b.name)
        && valid_id(&b.service_user_id)
        && b.conversations.len() <= 500
        && b.conversations.iter().all(|c| {
            ["group", "direct"].contains(&c.kind.as_str())
                && valid_id(&c.external_id)
                && valid_id(&c.project_id)
        })
}
fn fail() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "channel configuration unavailable").into_response()
}
async fn list(user: AuthUser, State(st): State<RagState>) -> Response {
    if !is_rag_admin(&user) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let rows=sqlx::query_scalar::<_,Value>(r#"SELECT jsonb_build_object('id',c.id,'name',c.name,'serviceUserId',c.service_user_id,'enabled',c.enabled,'allowBinding',c.allow_binding,
     'updatedAt',c.updated_at,'conversations',COALESCE((SELECT jsonb_agg(jsonb_build_object('kind',p.kind,'externalId',p.external_id,'projectId',p.project_id,'enabled',p.enabled) ORDER BY p.kind,p.external_id) FROM ms_rag_channel_conversation p WHERE p.channel_id=c.id),'[]'),
     'status',(SELECT jsonb_build_object('authenticated',s.authenticated,'connected',s.authenticated AND s.reported_at>now()-interval '90 seconds' AND COALESCE(s.heartbeat_age_seconds,999)<90,'heartbeatAgeSeconds',s.heartbeat_age_seconds,'activeQueries',s.active_queries,'lastMessageAt',s.last_message_at,'reportedAt',s.reported_at) FROM ms_rag_channel_status s WHERE s.channel_id=c.id),
     'pending',COALESCE((SELECT jsonb_agg(to_jsonb(x)) FROM (SELECT r.kind,r.external_id AS "externalId",r.sender_id AS "senderId",r.seen_at AS "seenAt" FROM ms_rag_channel_receipt r WHERE r.channel_id=c.id AND NOT EXISTS(SELECT 1 FROM ms_rag_channel_conversation p WHERE p.channel_id=c.id AND p.kind=r.kind AND p.external_id=r.external_id AND p.enabled) ORDER BY r.seen_at DESC LIMIT 100) x),'[]')) FROM ms_rag_channel c ORDER BY c.updated_at DESC"#).fetch_all(&st.pool).await;
    match rows {
        Ok(items) => Json(json!({"items":items})).into_response(),
        Err(_) => fail(),
    }
}
async fn create(user: AuthUser, State(st): State<RagState>, Json(b): Json<Policy>) -> Response {
    save(&user, &st, None, b).await
}
async fn update(
    user: AuthUser,
    State(st): State<RagState>,
    Path(id): Path<String>,
    Json(b): Json<Policy>,
) -> Response {
    save(&user, &st, Some(id), b).await
}
async fn save(user: &AuthUser, st: &RagState, id: Option<String>, b: Policy) -> Response {
    if !is_rag_admin(user) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !valid(&b) {
        return (StatusCode::BAD_REQUEST, "invalid channel policy").into_response();
    }
    let mut tx = match st.pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return fail(),
    };
    for p in &b.conversations {
        let member=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM ms_project_member m JOIN ms_project p ON p.id=m.project_id WHERE m.project_id=$1 AND m.user_id=$2 AND NOT p.deleted)").bind(&p.project_id).bind(&b.service_user_id).fetch_one(&mut *tx).await;
        if !matches!(member, Ok(true)) {
            return (
                StatusCode::BAD_REQUEST,
                "connector account must already belong to each authorized project",
            )
                .into_response();
        }
    }
    let channel=match id {
        Some(id)=>sqlx::query_scalar::<_,String>("UPDATE ms_rag_channel SET name=$2,service_user_id=$3,enabled=$4,updated_by=$5,allow_binding=$6,updated_at=now() WHERE id=$1 RETURNING id").bind(id).bind(&b.name).bind(&b.service_user_id).bind(b.enabled).bind(&user.user_id).bind(b.allow_binding).fetch_one(&mut *tx).await,
        None=>sqlx::query_scalar::<_,String>("INSERT INTO ms_rag_channel(name,service_user_id,enabled,updated_by,allow_binding) VALUES($1,$2,$3,$4,$5) RETURNING id").bind(&b.name).bind(&b.service_user_id).bind(b.enabled).bind(&user.user_id).bind(b.allow_binding).fetch_one(&mut *tx).await,
    };
    let channel = match channel {
        Ok(id) => id,
        Err(sqlx::Error::RowNotFound) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => {
            return (StatusCode::CONFLICT, "channel account is missing or already configured")
                .into_response()
        }
    };
    if sqlx::query("DELETE FROM ms_rag_channel_conversation WHERE channel_id=$1")
        .bind(&channel)
        .execute(&mut *tx)
        .await
        .is_err()
    {
        return fail();
    }
    for p in b.conversations {
        if sqlx::query("INSERT INTO ms_rag_channel_conversation(channel_id,kind,external_id,project_id,enabled) VALUES($1,$2,$3,$4,$5)").bind(&channel).bind(p.kind).bind(p.external_id).bind(p.project_id).bind(p.enabled).execute(&mut *tx).await.is_err(){return (StatusCode::BAD_REQUEST,"duplicate or invalid conversation").into_response()}
    }
    if tx.commit().await.is_err() {
        return fail();
    }
    Json(json!({"id":channel})).into_response()
}
async fn remove(user: AuthUser, State(st): State<RagState>, Path(id): Path<String>) -> Response {
    if !is_rag_admin(&user) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match sqlx::query("DELETE FROM ms_rag_channel WHERE id=$1").bind(id).execute(&st.pool).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => fail(),
    }
}
async fn runtime(user: AuthUser, State(st): State<RagState>) -> Response {
    if !user.can("RAG", "READ") {
        return StatusCode::FORBIDDEN.into_response();
    }
    let result=sqlx::query_scalar::<_,Value>(r#"SELECT jsonb_build_object('id',c.id,'enabled',c.enabled,'allowBinding',c.allow_binding,'conversations',COALESCE((SELECT jsonb_agg(jsonb_build_object('kind',p.kind,'externalId',p.external_id,'projectId',p.project_id,'enabled',p.enabled)) FROM ms_rag_channel_conversation p WHERE p.channel_id=c.id),'[]')) FROM ms_rag_channel c WHERE service_user_id=$1"#).bind(&user.user_id).fetch_optional(&st.pool).await;
    match result {
        Ok(Some(policy)) => Json(policy).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => fail(),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Report {
    authenticated: bool,
    heartbeat_age_seconds: Option<f64>,
    active_queries: u16,
    last_message_at: Option<i64>,
    receipt: Option<Receipt>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    kind: String,
    external_id: String,
    sender_id: String,
}
async fn report(user: AuthUser, State(st): State<RagState>, Json(b): Json<Report>) -> Response {
    if !user.can("RAG", "READ") {
        return StatusCode::FORBIDDEN.into_response();
    }
    if b.active_queries > 100
        || b.heartbeat_age_seconds.is_some_and(|v| !v.is_finite() || v < 0.0)
        || b.receipt.as_ref().is_some_and(|r| {
            !["group", "direct"].contains(&r.kind.as_str())
                || !valid_id(&r.external_id)
                || !valid_id(&r.sender_id)
        })
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let id = match sqlx::query_scalar::<_, String>(
        "SELECT id FROM ms_rag_channel WHERE service_user_id=$1",
    )
    .bind(&user.user_id)
    .fetch_optional(&st.pool)
    .await
    {
        Ok(Some(id)) => id,
        Ok(None) => return StatusCode::FORBIDDEN.into_response(),
        Err(_) => return fail(),
    };
    let mut tx = match st.pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return fail(),
    };
    if sqlx::query("INSERT INTO ms_rag_channel_status(channel_id,authenticated,heartbeat_age_seconds,active_queries,last_message_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT(channel_id) DO UPDATE SET authenticated=EXCLUDED.authenticated,heartbeat_age_seconds=EXCLUDED.heartbeat_age_seconds,active_queries=EXCLUDED.active_queries,last_message_at=EXCLUDED.last_message_at,reported_at=now()")
        .bind(&id).bind(b.authenticated).bind(b.heartbeat_age_seconds).bind(i32::from(b.active_queries)).bind(b.last_message_at).execute(&mut *tx).await.is_err(){return fail()}
    if let Some(r) = b.receipt {
        if sqlx::query("INSERT INTO ms_rag_channel_receipt(channel_id,kind,external_id,sender_id) VALUES($1,$2,$3,$4) ON CONFLICT(channel_id,kind,external_id) DO UPDATE SET sender_id=EXCLUDED.sender_id,seen_at=now()")
          .bind(&id).bind(r.kind).bind(r.external_id).bind(r.sender_id).execute(&mut *tx).await.is_err(){return fail()}
        if sqlx::query("DELETE FROM ms_rag_channel_receipt WHERE channel_id=$1 AND seen_at<now()-interval '30 days'").bind(&id).execute(&mut *tx).await.is_err(){return fail()}
    }
    if tx.commit().await.is_err() {
        return fail();
    }
    StatusCode::NO_CONTENT.into_response()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_rejects_unknown_types_and_empty_identifiers() {
        let mut p = Policy {
            name: "Connector".into(),
            service_user_id: "service".into(),
            enabled: true,
            allow_binding: false,
            conversations: vec![Conversation {
                kind: "direct".into(),
                external_id: "member".into(),
                project_id: "project".into(),
                enabled: true,
            }],
        };
        assert!(valid(&p));
        p.conversations[0].kind = "all".into();
        assert!(!valid(&p));
        p.conversations[0].kind = "group".into();
        p.conversations[0].external_id = "".into();
        assert!(!valid(&p));
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CodeRequest {
    project_id: String,
    expires_minutes: u32,
}
async fn binding_code(
    user: AuthUser,
    State(st): State<RagState>,
    Path(id): Path<String>,
    Json(b): Json<CodeRequest>,
) -> Response {
    if !is_rag_admin(&user) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !(1..=1440).contains(&b.expires_minutes) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let allowed=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM ms_rag_channel c JOIN ms_project_member m ON m.user_id=c.service_user_id WHERE c.id=$1 AND c.enabled AND c.allow_binding AND m.project_id=$2)").bind(&id).bind(&b.project_id).fetch_one(&st.pool).await;
    if !matches!(allowed, Ok(true)) {
        return (
            StatusCode::CONFLICT,
            "enable group binding and add the connector account to the project first",
        )
            .into_response();
    }
    let code = uuid::Uuid::new_v4().simple().to_string();
    let hash = format!("{:x}", Sha256::digest(code.as_bytes()));
    match sqlx::query("INSERT INTO ms_rag_channel_binding_code(code_hash,channel_id,project_id,expires_at,created_by) VALUES($1,$2,$3,now()+make_interval(mins=>$4),$5)").bind(hash).bind(id).bind(&b.project_id).bind(b.expires_minutes as i32).bind(user.user_id).execute(&st.pool).await {
        Ok(_)=>Json(json!({"code":code,"expiresMinutes":b.expires_minutes})).into_response(),Err(_)=>fail()
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Bind {
    code: String,
    chat_id: String,
}
async fn bind(user: AuthUser, State(st): State<RagState>, Json(b): Json<Bind>) -> Response {
    if !user.can("RAG", "READ") || !valid_id(&b.chat_id) || b.code.len() != 32 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let mut tx = match st.pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return fail(),
    };
    let hash = format!("{:x}", Sha256::digest(b.code.as_bytes()));
    let record=sqlx::query_as::<_,(String,String)>("SELECT k.channel_id,k.project_id FROM ms_rag_channel_binding_code k JOIN ms_rag_channel c ON c.id=k.channel_id WHERE k.code_hash=$1 AND k.used_at IS NULL AND k.expires_at>now() AND c.enabled AND c.allow_binding AND c.service_user_id=$2 AND EXISTS(SELECT 1 FROM ms_project_member m WHERE m.project_id=k.project_id AND m.user_id=c.service_user_id) FOR UPDATE OF k,c").bind(hash.clone()).bind(&user.user_id).fetch_optional(&mut *tx).await;
    let (channel, project) = match record {
        Ok(Some(v)) => v,
        _ => {
            return (StatusCode::FORBIDDEN, "binding disabled, expired, or already used")
                .into_response()
        }
    };
    if sqlx::query("INSERT INTO ms_rag_channel_conversation(channel_id,kind,external_id,project_id,enabled) VALUES($1,'group',$2,$3,true) ON CONFLICT(channel_id,kind,external_id) DO UPDATE SET project_id=EXCLUDED.project_id,enabled=true").bind(channel).bind(b.chat_id).bind(project).execute(&mut *tx).await.is_err(){return fail()}
    if sqlx::query("UPDATE ms_rag_channel_binding_code SET used_at=now() WHERE code_hash=$1")
        .bind(hash)
        .execute(&mut *tx)
        .await
        .is_err()
    {
        return fail();
    }
    if tx.commit().await.is_err() {
        return fail();
    }
    Json(json!({"bound":true})).into_response()
}

#[cfg(test)]
mod integration_tests {
    use super::super::test_support::{body, fixture};
    use super::*;
    #[tokio::test]
    #[ignore = "requires isolated SHEPHERD_RAG_TEST_DATABASE_URL"]
    async fn backend_gates_binding_and_one_use_codes_and_runtime_identity() {
        let (st, mut admin, project, _) = fixture().await;
        sqlx::query("INSERT INTO ms_user(id,name,email) VALUES($1,'Connector fixture',$2)")
            .bind(&admin.user_id)
            .bind(format!("{}@example.invalid", admin.user_id))
            .execute(&st.pool)
            .await
            .unwrap();
        let ordinary = admin.clone();
        admin.permissions =
            webauth::PermissionSet::from_raw(["SYSTEM_USER:UPDATE", "RAG:READ"]).unwrap();
        let policy = || Policy {
            name: "Connector fixture".into(),
            service_user_id: admin.user_id.clone(),
            enabled: true,
            allow_binding: false,
            conversations: vec![],
        };
        assert_eq!(list(ordinary.clone(), State(st.clone())).await.status(), StatusCode::FORBIDDEN);
        let id = body(create(admin.clone(), State(st.clone()), Json(policy())).await).await["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            binding_code(
                admin.clone(),
                State(st.clone()),
                Path(id.clone()),
                Json(CodeRequest { project_id: project.clone(), expires_minutes: 30 })
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );
        let mut p = policy();
        p.allow_binding = true;
        body(update(admin.clone(), State(st.clone()), Path(id.clone()), Json(p)).await).await;
        let code = body(
            binding_code(
                admin.clone(),
                State(st.clone()),
                Path(id.clone()),
                Json(CodeRequest { project_id: project.clone(), expires_minutes: 30 }),
            )
            .await,
        )
        .await["code"]
            .as_str()
            .unwrap()
            .to_owned();
        let request = || Bind { code: code.clone(), chat_id: "test-group".into() };
        body(bind(ordinary.clone(), State(st.clone()), Json(request())).await).await;
        assert_eq!(
            bind(ordinary.clone(), State(st.clone()), Json(request())).await.status(),
            StatusCode::FORBIDDEN
        );
        let runtime_value = body(runtime(ordinary.clone(), State(st.clone())).await).await;
        assert_eq!(runtime_value["conversations"][0]["projectId"], project);
        let mut outsider = ordinary.clone();
        outsider.user_id = uuid::Uuid::new_v4().to_string();
        assert_eq!(runtime_for(outsider, st.clone()).await, StatusCode::NOT_FOUND);
        assert_eq!(
            report(
                ordinary.clone(),
                State(st.clone()),
                Json(Report {
                    authenticated: true,
                    heartbeat_age_seconds: Some(0.0),
                    active_queries: 1,
                    last_message_at: Some(now_ms()),
                    receipt: Some(Receipt {
                        kind: "direct".into(),
                        external_id: "unapproved-user".into(),
                        sender_id: "unapproved-user".into()
                    })
                })
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
        let rows = body(list(admin.clone(), State(st.clone())).await).await;
        let own = rows["items"].as_array().unwrap().iter().find(|r| r["id"] == id).unwrap();
        assert_eq!(own["status"]["connected"], true);
        assert_eq!(own["pending"][0]["externalId"], "unapproved-user");
        let mut p = policy();
        p.enabled = false;
        body(update(admin.clone(), State(st.clone()), Path(id), Json(p)).await).await;
        assert_eq!(body(runtime(ordinary, State(st)).await).await["enabled"], false);
    }
    async fn runtime_for(user: AuthUser, st: RagState) -> StatusCode {
        runtime(user, State(st)).await.status()
    }
}
