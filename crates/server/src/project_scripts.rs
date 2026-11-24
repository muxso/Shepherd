use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct ScriptState {
    pool: PgPool,
    sessions: Arc<dyn SessionStore>,
    client: reqwest::Client,
}

impl FromRef<ScriptState> for Arc<dyn SessionStore> {
    fn from_ref(state: &ScriptState) -> Self {
        state.sessions.clone()
    }
}

pub fn router(pool: PgPool, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/project/{project}/scripts", get(list).post(create))
        .route("/project/{project}/scripts/{id}", get(history).put(update))
        .route("/project/{project}/scripts/{id}/revisions/{revision}", get(source))
        .route("/project/{project}/scripts/debug", post(debug_draft))
        .route("/project/{project}/scripts/{id}/revisions/{revision}/debug", post(debug_saved))
        .with_state(ScriptState { pool, sessions, client: reqwest::Client::new() })
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct Script {
    id: String,
    name: String,
    language: String,
    revision: i32,
    updated_at: String,
    description: String,
    tags: Vec<String>,
    test_status: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct Revision {
    revision: i32,
    name: String,
    created_by: String,
    created_at: String,
}

#[derive(Deserialize)]
struct Draft {
    name: String,
    source: String,
    language: String,
    revision: Option<i32>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    parameters: Vec<Parameter>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Parameter {
    name: String,
    #[serde(default)]
    default_value: Option<serde_json::Value>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    required: bool,
}

#[derive(Deserialize)]
struct DebugDraft {
    language: String,
    source: String,
    input: serde_json::Value,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DebugResult {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
    duration_ms: u64,
    output_limited: bool,
}

async fn execute(client: &reqwest::Client, draft: DebugDraft) -> Result<DebugResult, StatusCode> {
    if !matches!(draft.language.as_str(), "python" | "javascript")
        || draft.source.trim().is_empty()
        || draft.source.len() > 65_536
        || !draft.input.is_object()
        || draft.input.to_string().len() > 65_536
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let endpoint =
        std::env::var("SHEPHERD_SCRIPT_RUNNER_URL").map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let token = std::env::var("SHEPHERD_SCRIPT_RUNNER_TOKEN")
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let response = client
        .post(endpoint)
        .bearer_auth(token)
        .timeout(std::time::Duration::from_secs(9))
        .json(&serde_json::json!({
            "language": draft.language, "source": draft.source, "input": draft.input
        }))
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    response
        .error_for_status()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .json()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)
}

pub(crate) async fn run_processor(
    pool: &PgPool,
    user: &AuthUser,
    project: &str,
    processor: &api_runner::Processor,
    variables: &mut std::collections::BTreeMap<String, String>,
) -> Result<(), StatusCode> {
    use api_runner::{Processor, ScriptSource};
    let Processor::SandboxScript { source, input, output } = processor else {
        return Err(StatusCode::BAD_REQUEST);
    };
    authorize_project(pool, user, project, "EXECUTE").await?;
    let mut name = output.bytes();
    if output.len() > 64
        || output.starts_with("__")
        || !name.next().is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        || !name.all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (language, source, parameters) = match source {
        ScriptSource::Custom { language, code } => {
            (language.clone(), code.clone(), Vec::<Parameter>::new())
        }
        ScriptSource::Shared { script_id, revision } => {
            let (language, source, parameters): (String, String, serde_json::Value) =
                sqlx::query_as(
                    "SELECT s.language,r.source,r.parameters FROM ms_project_script_revision r
                 JOIN ms_project_script s ON s.id=r.script_id
                 WHERE s.project_id=$1 AND s.id=$2 AND r.revision=$3",
                )
                .bind(project)
                .bind(script_id)
                .bind(revision)
                .fetch_one(pool)
                .await
                .map_err(storage)?;
            (
                language,
                source,
                serde_json::from_value(parameters)
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            )
        }
    };
    let mut input = input.as_object().cloned().ok_or(StatusCode::BAD_REQUEST)?;
    for parameter in parameters {
        if !input.contains_key(&parameter.name) {
            if let Some(value) = parameter.default_value {
                input.insert(parameter.name, value);
            } else if parameter.required {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }
    for value in input.values_mut() {
        if let Some(text) = value.as_str() {
            if text.contains("{{") || text.contains("${") {
                *value = serde_json::Value::String(
                    api_runner::domain::preparation::render(text, variables, 65_536)
                        .map_err(|_| StatusCode::BAD_REQUEST)?,
                );
            }
        }
    }
    let result = execute(
        &reqwest::Client::new(),
        DebugDraft { language, source, input: serde_json::Value::Object(input) },
    )
    .await?;
    if result.exit_code != Some(0) || result.output_limited {
        return Err(if result.timed_out {
            StatusCode::GATEWAY_TIMEOUT
        } else {
            StatusCode::UNPROCESSABLE_ENTITY
        });
    }
    let value: serde_json::Value =
        serde_json::from_str(&result.stdout).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    variables.insert(output.clone(), value.to_string());
    Ok(())
}

async fn debug_draft(
    user: AuthUser,
    State(state): State<ScriptState>,
    Path(project): Path<String>,
    Json(draft): Json<DebugDraft>,
) -> Result<Json<DebugResult>, StatusCode> {
    authorize(&state, &user, &project, "EXECUTE").await?;
    Ok(Json(execute(&state.client, draft).await?))
}

async fn debug_saved(
    user: AuthUser,
    State(state): State<ScriptState>,
    Path((project, id, revision)): Path<(String, String, i32)>,
    Json(input): Json<serde_json::Value>,
) -> Result<Json<DebugResult>, StatusCode> {
    authorize(&state, &user, &project, "EXECUTE").await?;
    let (language, source): (String, String) = sqlx::query_as(
        "SELECT s.language,r.source FROM ms_project_script_revision r
         JOIN ms_project_script s ON s.id=r.script_id
         WHERE s.project_id=$1 AND s.id=$2 AND r.revision=$3",
    )
    .bind(project)
    .bind(&id)
    .bind(revision)
    .fetch_one(&state.pool)
    .await
    .map_err(storage)?;
    let result = execute(&state.client, DebugDraft { language, source, input }).await?;
    let status =
        if result.exit_code == Some(0) && !result.output_limited { "passed" } else { "failed" };
    sqlx::query(
        "UPDATE ms_project_script_revision SET test_status=$3 WHERE script_id=$1 AND revision=$2",
    )
    .bind(id)
    .bind(revision)
    .bind(status)
    .execute(&state.pool)
    .await
    .map_err(storage)?;
    Ok(Json(result))
}

fn storage(error: sqlx::Error) -> StatusCode {
    if error.as_database_error().is_some_and(|e| e.is_unique_violation()) {
        return StatusCode::CONFLICT;
    }
    if matches!(error, sqlx::Error::RowNotFound) {
        return StatusCode::NOT_FOUND;
    }
    tracing::error!(%error, "Could not access project scripts");
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn authorize(
    state: &ScriptState,
    user: &AuthUser,
    project: &str,
    action: &str,
) -> Result<(), StatusCode> {
    authorize_project(&state.pool, user, project, action).await
}

async fn authorize_project(
    pool: &PgPool,
    user: &AuthUser,
    project: &str,
    action: &str,
) -> Result<(), StatusCode> {
    let admin = user.can("SYSTEM_USER", "UPDATE");
    if !user.can("SCRIPT", action) && !admin {
        return Err(StatusCode::FORBIDDEN);
    }
    let allowed: bool = sqlx::query_scalar("SELECT ms_has_project_access($1, $2, $3)")
        .bind(project)
        .bind(&user.user_id)
        .bind(admin)
        .fetch_one(pool)
        .await
        .map_err(storage)?;
    allowed.then_some(()).ok_or(StatusCode::NOT_FOUND)
}

async fn list(
    user: AuthUser,
    State(state): State<ScriptState>,
    Path(project): Path<String>,
) -> Result<Json<Vec<Script>>, StatusCode> {
    authorize(&state, &user, &project, "READ").await?;
    let scripts = sqlx::query_as(
        "SELECT s.id,s.name,s.language,s.revision,s.updated_at::text,r.description,r.tags,r.test_status
         FROM ms_project_script s JOIN ms_project_script_revision r
         ON r.script_id=s.id AND r.revision=s.revision WHERE s.project_id=$1 ORDER BY s.updated_at DESC,s.id",
    )
    .bind(project)
    .fetch_all(&state.pool)
    .await
    .map_err(storage)?;
    Ok(Json(scripts))
}

async fn history(
    user: AuthUser,
    State(state): State<ScriptState>,
    Path((project, id)): Path<(String, String)>,
) -> Result<Json<Vec<Revision>>, StatusCode> {
    authorize(&state, &user, &project, "READ").await?;
    let revisions: Vec<Revision> = sqlx::query_as(
        "SELECT r.revision,r.name,r.created_by,r.created_at::text
         FROM ms_project_script_revision r JOIN ms_project_script s ON s.id=r.script_id
         WHERE s.project_id=$1 AND s.id=$2 ORDER BY r.revision DESC LIMIT 50",
    )
    .bind(project)
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(storage)?;
    if revisions.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(revisions))
}

async fn source(
    user: AuthUser,
    State(state): State<ScriptState>,
    Path((project, id, revision)): Path<(String, String, i32)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    authorize(&state, &user, &project, "READ").await?;
    let (source, description, tags, parameters, test_status): (String, String, Vec<String>, serde_json::Value, Option<String>) = sqlx::query_as(
        "SELECT r.source,r.description,r.tags,r.parameters,r.test_status FROM ms_project_script_revision r JOIN ms_project_script s ON s.id=r.script_id
         WHERE s.project_id=$1 AND s.id=$2 AND r.revision=$3",
    )
    .bind(project)
    .bind(id)
    .bind(revision)
    .fetch_one(&state.pool)
    .await
    .map_err(storage)?;
    Ok(Json(
        serde_json::json!({ "source": source, "description": description, "tags": tags, "parameters": parameters, "testStatus": test_status }),
    ))
}

async fn create(
    user: AuthUser,
    State(state): State<ScriptState>,
    Path(project): Path<String>,
    Json(draft): Json<Draft>,
) -> Result<(StatusCode, Json<Script>), StatusCode> {
    authorize(&state, &user, &project, "ADD").await?;
    Ok((StatusCode::CREATED, Json(save(&state, &user, &project, None, draft).await?)))
}

async fn update(
    user: AuthUser,
    State(state): State<ScriptState>,
    Path((project, id)): Path<(String, String)>,
    Json(draft): Json<Draft>,
) -> Result<Json<Script>, StatusCode> {
    authorize(&state, &user, &project, "UPDATE").await?;
    Ok(Json(save(&state, &user, &project, Some(id), draft).await?))
}

async fn save(
    state: &ScriptState,
    user: &AuthUser,
    project: &str,
    id: Option<String>,
    draft: Draft,
) -> Result<Script, StatusCode> {
    let name = draft.name.trim();
    if name.is_empty()
        || name.chars().count() > 120
        || draft.source.trim().is_empty()
        || draft.source.len() > 65_536
        || !matches!(draft.language.as_str(), "python" | "javascript")
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut names = std::collections::HashSet::new();
    if draft.description.chars().count() > 1000
        || draft.tags.len() > 20
        || draft.tags.iter().any(|tag| tag.trim().is_empty() || tag.chars().count() > 40)
        || draft.parameters.len() > 30
        || draft.parameters.iter().any(|p| {
            p.name.is_empty()
                || p.name.chars().count() > 64
                || p.name.trim() != p.name
                || p.description.chars().count() > 200
                || !names.insert(&p.name)
        })
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let parameters =
        serde_json::to_value(&draft.parameters).map_err(|_| StatusCode::BAD_REQUEST)?;
    if parameters.to_string().len() > 65_536 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut tx = state.pool.begin().await.map_err(storage)?;
    let mut script: Script = if let Some(id) = id {
        let expected = draft.revision.ok_or(StatusCode::BAD_REQUEST)?;
        sqlx::query_as(
            "UPDATE ms_project_script SET name=$3,revision=revision+1,updated_at=now()
             WHERE project_id=$1 AND id=$2 AND revision=$4 AND language=$5
             RETURNING id,name,language,revision,updated_at::text,''::text AS description,'{}'::text[] AS tags,NULL::text AS test_status",
        )
        .bind(project).bind(id).bind(name).bind(expected).bind(&draft.language)
        .fetch_optional(&mut *tx).await.map_err(storage)?.ok_or(StatusCode::CONFLICT)?
    } else {
        sqlx::query_as(
            "INSERT INTO ms_project_script(project_id,name,language) VALUES($1,$2,$3)
             RETURNING id,name,language,revision,updated_at::text,''::text AS description,'{}'::text[] AS tags,NULL::text AS test_status",
        )
        .bind(project).bind(name).bind(&draft.language)
        .fetch_one(&mut *tx).await.map_err(storage)?
    };
    sqlx::query(
        "INSERT INTO ms_project_script_revision(script_id,revision,name,source,created_by,description,tags,parameters)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(&script.id).bind(script.revision).bind(name).bind(draft.source).bind(&user.user_id).bind(&draft.description).bind(&draft.tags).bind(parameters)
    .execute(&mut *tx).await.map_err(storage)?;
    tx.commit().await.map_err(storage)?;
    script.description = draft.description;
    script.tags = draft.tags;
    Ok(script)
}
