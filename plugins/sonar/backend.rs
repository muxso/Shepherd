use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{
    extract::{FromRef, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct SonarState {
    pool: PgPool,
    identity_pool: Option<PgPool>,
    host_pool: PgPool,
    client: reqwest::Client,
    sessions: Arc<dyn SessionStore>,
    notifier: notice::application::Notifier,
}
impl FromRef<SonarState> for Arc<dyn SessionStore> {
    fn from_ref(state: &SonarState) -> Self {
        state.sessions.clone()
    }
}

pub async fn connect(host: &PgPool) -> Result<PgPool, Box<dyn std::error::Error>> {
    let url = std::env::var("SHEPHERD_SONAR_DATABASE_URL")?;
    let pool = PgPool::connect(&url).await?;
    let host_name: String =
        sqlx::query_scalar("SELECT current_database()::text").fetch_one(host).await?;
    let sonar_name: String =
        sqlx::query_scalar("SELECT current_database()::text").fetch_one(&pool).await?;
    if host_name == sonar_name {
        return Err("Sonar must use a different database from Shepherd".into());
    }
    sqlx::raw_sql(include_str!("schema.sql")).execute(&pool).await?;
    Ok(pool)
}

pub fn router(
    pool: PgPool,
    sessions: Arc<dyn SessionStore>,
    identity_pool: Option<PgPool>,
    notifier: notice::application::Notifier,
    host_pool: PgPool,
) -> Router {
    Router::new()
        .route("/api/sonar/sources/{id}/analyses", get(list).post(ingest))
        .route("/api/sonar/dashboard", get(dashboard))
        .route(
            "/api/sonar/sources/{id}/notification-settings",
            get(notification_settings).put(save_notification_settings),
        )
        .route(
            "/api/sonar/sources/{id}/notification-settings/runtime",
            post(runtime_notification_settings),
        )
        .route("/api/sonar/sources/{id}/project-notification", post(project_notification))
        .route("/api/sonar/sources/{id}/analyses/{task}/diff", get(commit_diff))
        .route("/api/sonar/sources/{id}/analyses/{task}/issues", get(analysis_issues))
        .with_state(SonarState {
            pool,
            identity_pool,
            host_pool,
            sessions,
            notifier,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("valid Sonar HTTP client"),
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotificationSettings {
    dashboard_base: Option<String>,
    message_template: Option<Value>,
    webhook_url: Option<String>,
    #[serde(default)]
    reset_webhook: bool,
}

async fn load_notification_settings(
    pool: &PgPool,
    source: &str,
) -> Result<(Option<String>, Option<String>, Option<Value>), StatusCode> {
    if invalid_key(source) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(sqlx::query_as(
        "SELECT dashboard_base, webhook_url, message_template
         FROM ms_sonar_notification_settings WHERE source_id=$1",
    )
    .bind(source)
    .fetch_optional(pool)
    .await
    .map_err(storage)?
    .unwrap_or_default())
}

async fn notification_settings(
    user: AuthUser,
    State(state): State<SonarState>,
    Path(source): Path<String>,
) -> Result<Response, StatusCode> {
    if !user.can("SYSTEM_USER", "UPDATE") {
        return Err(StatusCode::FORBIDDEN);
    }
    let (dashboard, webhook, template) = load_notification_settings(&state.pool, &source).await?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({
            "dashboardBase": dashboard,
            "webhookConfigured": webhook.is_some(),
            "messageTemplate": template,
        })),
    )
        .into_response())
}

async fn runtime_notification_settings(
    State(state): State<SonarState>,
    Path(source): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    authenticate_notifier(&headers)?;
    let (dashboard, webhook, template) = load_notification_settings(&state.pool, &source).await?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(
            json!({"dashboardBase": dashboard, "webhookUrl": webhook, "messageTemplate": template}),
        ),
    )
        .into_response())
}

fn authenticate_notifier(headers: &HeaderMap) -> Result<(), StatusCode> {
    let key = std::env::var("SHEPHERD_SONAR_NOTIFY_TOKEN")
        .ok()
        .filter(|key| key.len() == 64)
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|token| token.len() == key.len())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let difference = supplied
        .bytes()
        .zip(key.bytes())
        .fold(0u8, |difference, (left, right)| difference | (left ^ right));
    if difference != 0 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectNotification {
    task_id: String,
    title: String,
    content: String,
    operator: String,
}

async fn project_notification(
    State(state): State<SonarState>,
    Path(source): Path<String>,
    headers: HeaderMap,
    Json(event): Json<ProjectNotification>,
) -> Result<StatusCode, StatusCode> {
    authenticate_notifier(&headers)?;
    if invalid_key(&source)
        || invalid_key(&event.task_id)
        || event.title.trim().is_empty()
        || event.title.len() > 256
        || event.content.len() > 8192
        || event.operator.len() > 256
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let project = match std::env::var("SHEPHERD_SONAR_NOTIFICATION_PROJECT_ID") {
        Ok(project) if !invalid_key(&project) => Some(project),
        Ok(_) => return Err(StatusCode::SERVICE_UNAVAILABLE),
        Err(_) => match &state.identity_pool {
            Some(pool) => sqlx::query_scalar("SELECT project_id FROM ms_devops_source WHERE id=$1")
                .bind(source)
                .fetch_optional(pool)
                .await
                .map_err(storage)?,
            None => None,
        },
    };
    let Some(project_id) = project else { return Ok(StatusCode::NOT_FOUND) };
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM ms_project WHERE id=$1 AND enable AND NOT deleted)",
    )
    .bind(&project_id)
    .fetch_one(&state.host_pool)
    .await
    .map_err(storage)?;
    if !active {
        return Ok(StatusCode::NOT_FOUND);
    }
    let handled = state
        .notifier
        .notify_project_if_configured(notice::application::NoticeEvent {
            project_id,
            category: "SONAR".into(),
            event_type: "SONAR_ANALYSIS_COMPLETED".into(),
            title: event.title,
            content: event.content,
            resource_type: "SONAR_ANALYSIS".into(),
            resource_id: event.task_id,
            operator: event.operator,
        })
        .await
        .map_err(|error| {
            tracing::warn!(%error, "Sonar project notification failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(if handled { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
}

fn valid_notification_url(value: &str, webhook: bool) -> bool {
    if value.len() > 2048 || value.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return false;
    }
    let Ok(url) = reqwest::Url::parse(value) else { return false };
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    if !webhook {
        return matches!(url.scheme(), "http" | "https") && url.query().is_none();
    }
    let mut pairs = url.query_pairs();
    url.scheme() == "https"
        && url.host_str() == Some("qyapi.weixin.qq.com")
        && url.port().is_none()
        && url.path() == "/cgi-bin/webhook/send"
        && pairs.next().is_some_and(|(name, key)| name == "key" && !key.is_empty())
        && pairs.next().is_none()
}

fn valid_template_variables(value: &Value, placeholder: &regex::Regex) -> bool {
    match value {
        Value::Object(items) => {
            items.values().all(|item| valid_template_variables(item, placeholder))
        }
        Value::Array(items) => items.iter().all(|item| valid_template_variables(item, placeholder)),
        Value::String(text) => placeholder.captures_iter(text).all(|capture| {
            matches!(&capture[1], "project" | "status" | "summary" | "rows" | "color" | "dashboard")
        }),
        _ => true,
    }
}

fn valid_notification_template(template: &Value) -> bool {
    static PLACEHOLDER: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\$\{([^{}]*)\}").expect("valid placeholder regex")
    });
    let valid_shape = match template["msgtype"].as_str() {
        Some("text") => nonempty_string(&template["text"]["content"]),
        Some("markdown") => nonempty_string(&template["markdown"]["content"]),
        Some("template_card") => {
            let card = &template["template_card"];
            matches!(card["card_type"].as_str(), Some("text_notice" | "news_notice"))
                && nonempty_string(&card["main_title"]["title"])
                && card["card_action"]["type"].as_u64().is_some()
        }
        _ => false,
    };
    valid_shape
        && template.to_string().len() <= 16_384
        && valid_template_variables(template, &PLACEHOLDER)
}

fn nonempty_string(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.trim().is_empty())
}

async fn save_notification_settings(
    user: AuthUser,
    State(state): State<SonarState>,
    Path(source): Path<String>,
    Json(settings): Json<NotificationSettings>,
) -> Result<StatusCode, (StatusCode, &'static str)> {
    if !user.can("SYSTEM_USER", "UPDATE") {
        return Err((StatusCode::FORBIDDEN, "Permission denied"));
    }
    if invalid_key(&source) {
        return Err((StatusCode::BAD_REQUEST, "Invalid SonarQube source"));
    }
    if settings.dashboard_base.as_deref().is_some_and(|url| !valid_notification_url(url, false)) {
        return Err((StatusCode::BAD_REQUEST, "Invalid SonarQube base URL"));
    }
    if settings.webhook_url.as_deref().is_some_and(|url| !valid_notification_url(url, true))
        || (settings.webhook_url.is_some() && settings.reset_webhook)
    {
        return Err((StatusCode::BAD_REQUEST, "Invalid WeCom webhook URL"));
    }
    if settings
        .message_template
        .as_ref()
        .is_some_and(|template| !valid_notification_template(template))
    {
        return Err((StatusCode::BAD_REQUEST, "Invalid WeCom template or template variable"));
    }
    sqlx::query(
        "INSERT INTO ms_sonar_notification_settings (source_id, dashboard_base, webhook_url, message_template)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (source_id) DO UPDATE SET dashboard_base=EXCLUDED.dashboard_base,
           webhook_url=CASE WHEN $5 THEN NULL ELSE coalesce(EXCLUDED.webhook_url, ms_sonar_notification_settings.webhook_url) END,
           message_template=EXCLUDED.message_template",
    )
    .bind(source)
    .bind(settings.dashboard_base)
    .bind(settings.webhook_url)
    .bind(settings.message_template)
    .bind(settings.reset_webhook)
    .execute(&state.pool)
    .await
    .map_err(|error| (storage(error), "Could not save notification settings"))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    scope: String,
    task_id: String,
    repository_id: String,
    project_key: String,
    pull_request: Option<String>,
    revision: String,
    analyzed_at: String,
}

fn invalid_key(value: &str) -> bool {
    value.is_empty()
        || value.len() > 256
        || value.trim() != value
        || value.chars().any(char::is_control)
}

fn valid_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && value.bytes().any(|byte| byte != b'0')
}

async fn ingest(
    user: AuthUser,
    State(state): State<SonarState>,
    Path(source): Path<String>,
    Json(report): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    if !user.can("DEVOPS", "SYNC") && !user.can("PROJECT", "UPDATE") {
        return Err(StatusCode::FORBIDDEN);
    }
    let meta: Report =
        serde_json::from_value(report.clone()).map_err(|_| StatusCode::BAD_REQUEST)?;
    let analyzed_at = chrono::DateTime::parse_from_rfc3339(&meta.analyzed_at)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let pr = meta.pull_request.as_deref().unwrap_or_default();
    if [&source, &meta.task_id, &meta.repository_id, &meta.project_key]
        .into_iter()
        .any(|key| invalid_key(key))
        || !valid_sha(&meta.revision)
        || !matches!(
            (meta.scope.as_str(), pr.is_empty()),
            ("sonar_repository", true) | ("ci_pull_request", false)
        )
        || !matches!(report["gate"].as_str(), Some("OK" | "ERROR" | "WARN" | "NONE"))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    if let Some(push) = report.get("push").filter(|value| !value.is_null()) {
        let before = push["baseRevision"].as_str().unwrap_or_default();
        if meta.scope != "sonar_repository"
            || !valid_sha(before)
            || before.eq_ignore_ascii_case(&meta.revision)
            || invalid_key(push["branch"].as_str().unwrap_or_default())
            || push["pushedAt"]
                .as_str()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                .is_none()
            || push["author"].as_str().is_some_and(invalid_key)
            || push.get("baselineVerified").is_some_and(|value| !value.is_boolean())
        {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let insert = sqlx::query(
        "INSERT INTO ms_sonar_analysis (source_id,scope,task_id,repository_id,pull_request,revision,analyzed_at,report)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    ).bind(source).bind(meta.scope).bind(meta.task_id).bind(meta.repository_id).bind(pr)
        .bind(meta.revision.to_ascii_lowercase()).bind(analyzed_at.to_utc()).bind(report)
        .execute(&state.pool).await.map_err(storage)?;
    Ok(Json(json!({"inserted": insert.rows_affected() == 1})))
}

fn can_read_quality(user: &AuthUser) -> bool {
    user.can("SONAR", "READ") || user.can("PROJECT", "UPDATE")
}

#[derive(Deserialize)]
struct DashboardQuery {
    source: Option<String>,
    days: Option<i32>,
}

fn quality_view(report: &mut Value) {
    if let Some(fields) = report.as_object_mut() {
        fields.retain(|key, _| {
            matches!(
                key.as_str(),
                "scope"
                    | "taskId"
                    | "projectKey"
                    | "repositoryId"
                    | "repositoryName"
                    | "branch"
                    | "pullRequest"
                    | "revision"
                    | "author"
                    | "commitAuthor"
                    | "analyzedAt"
                    | "gate"
                    | "newIssues"
                    | "newCoverage"
                    | "newDuplication"
                    | "push"
                    | "newCode"
                    | "analysisQuality"
                    | "overallQuality"
                    | "repositoryUrl"
                    | "sonarUrl"
            )
        });
    }
}

async fn dashboard(
    user: AuthUser,
    State(state): State<SonarState>,
    Query(query): Query<DashboardQuery>,
) -> Result<Json<Value>, StatusCode> {
    if !can_read_quality(&user) {
        return Err(StatusCode::FORBIDDEN);
    }
    let can_manage_notifications = user.can("SYSTEM_USER", "UPDATE");
    let days = query.days.unwrap_or(30);
    if !matches!(days, 7 | 30 | 90) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let source_ids: Vec<String> = sqlx::query_scalar(
        "SELECT source_id FROM ms_sonar_analysis GROUP BY source_id ORDER BY max(analyzed_at) DESC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(storage)?;
    let sources: Vec<_> = source_ids.iter().enumerate()
        .map(|(index, id)| json!({"id":id,"name": if source_ids.len() == 1 { "SonarQube".to_string() } else { format!("SonarQube {}", index+1) }}))
        .collect();
    let Some(source) = query.source.as_ref().or_else(|| source_ids.first()) else {
        return Ok(Json(
            json!({"sources":sources,"source":null,"items":[],"truncated":false,"canManageNotifications":can_manage_notifications}),
        ));
    };
    if !source_ids.contains(source) {
        return Err(StatusCode::NOT_FOUND);
    }
    let mut items: Vec<Value> = sqlx::query_scalar(
        "SELECT report FROM ms_sonar_analysis WHERE source_id=$1 AND analyzed_at >=
         (((now() AT TIME ZONE 'Asia/Shanghai')::date - ($2 - 1))::timestamp AT TIME ZONE 'Asia/Shanghai')
         ORDER BY analyzed_at DESC,task_id LIMIT 1001",
    ).bind(source).bind(days).fetch_all(&state.pool).await.map_err(storage)?;
    let truncated = items.len() > 1000;
    items.truncate(1000);
    items.iter_mut().for_each(quality_view);
    if let Some(identity_pool) = &state.identity_pool {
        let aliases: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
            "SELECT alias, name FROM ms_devops_author_alias WHERE source_id=$1",
        )
        .bind(source)
        .fetch_all(identity_pool)
        .await
        .map_err(storage)?
        .into_iter()
        .collect();
        for report in &mut items {
            let author = if report["scope"] == "ci_pull_request" {
                report["author"].as_str()
            } else {
                report["push"]["author"]
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .or_else(|| report["commitAuthor"].as_str())
            };
            if let Some(name) = author.and_then(|author| aliases.get(author.trim())) {
                report["personName"] = json!(name);
            }
        }
    }
    Ok(Json(
        json!({"sources":sources,"source":source,"items":items,"truncated":truncated,"canManageNotifications":can_manage_notifications}),
    ))
}

async fn archived_analysis(
    state: &SonarState,
    source: &str,
    task: &str,
) -> Result<Value, StatusCode> {
    sqlx::query_scalar(
        "SELECT report FROM ms_sonar_analysis WHERE source_id=$1 AND task_id=$2 AND scope='sonar_repository'",
    ).bind(source).bind(task).fetch_optional(&state.pool).await.map_err(storage)?
        .ok_or(StatusCode::NOT_FOUND)
}

async fn upstream_json(request: reqwest::RequestBuilder) -> Result<Value, StatusCode> {
    let mut response = request
        .timeout(Duration::from_secs(12))
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            404 => StatusCode::NOT_FOUND,
            _ => StatusCode::BAD_GATEWAY,
        });
    }
    const MAX_BYTES: usize = 6 * 1024 * 1024;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| StatusCode::BAD_GATEWAY)? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_GATEWAY)
}

fn no_store(payload: Value) -> impl IntoResponse {
    ([(header::CACHE_CONTROL, "private, no-store")], Json(payload))
}

async fn commit_diff(
    user: AuthUser,
    State(state): State<SonarState>,
    Path((source, task)): Path<(String, String)>,
) -> Result<impl IntoResponse, StatusCode> {
    if !user.can("SONAR_CODE", "READ") && !user.can("SYSTEM_USER", "UPDATE") {
        return Err(StatusCode::FORBIDDEN);
    }
    if !can_read_quality(&user) {
        return Err(StatusCode::FORBIDDEN);
    }
    let report = archived_analysis(&state, &source, &task).await?;
    let token = std::env::var("SHEPHERD_SONAR_CODEUP_TOKEN")
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let org =
        std::env::var("SHEPHERD_SONAR_CODEUP_ORG").map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let repository = report["repositoryId"].as_str().unwrap_or_default();
    let revision = report["revision"].as_str().unwrap_or_default();
    if !valid_sha(revision)
        || repository.is_empty()
        || !repository.bytes().all(|byte| byte.is_ascii_digit())
        || org.is_empty()
        || !org.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let base = format!("https://openapi-rdc.aliyuncs.com/oapi/v1/codeup/organizations/{org}/repositories/{repository}");
    let commit = upstream_json(
        state.client.get(format!("{base}/commits/{revision}")).header("x-yunxiao-token", &token),
    )
    .await?;
    if commit["id"].as_str() != Some(revision) {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let Some(parent) =
        commit["parentIds"].get(0).and_then(Value::as_str).filter(|value| valid_sha(value))
    else {
        return Ok(no_store(
            json!({"revision":revision,"title":commit["title"],"status":"no_parent","files":[]}),
        ));
    };
    let diff = upstream_json(
        state.client.get(format!("{base}/compares")).header("x-yunxiao-token", &token).query(&[
            ("from", parent),
            ("to", revision),
            ("straight", "true"),
        ]),
    )
    .await?;
    let files = diff["diffs"].as_array().ok_or(StatusCode::BAD_GATEWAY)?;
    let limit = 50;
    let mut truncated = files.len() > limit;
    let files: Vec<_> = files
        .iter()
        .take(limit)
        .map(|file| {
            let patch = file["diff"].as_str().unwrap_or_default();
            let preview: String = patch.chars().take(12_000).collect();
            let clipped = preview.len() < patch.len();
            truncated |= clipped;
            json!({"path":file["newPath"],"oldPath":file["oldPath"],"binary":file["isBinary"],
            "diff":preview,"clipped":clipped,"added":file["newFile"],"deleted":file["deletedFile"]})
        })
        .collect();
    Ok(no_store(
        json!({"revision":revision,"title":commit["title"],"parent":parent,"status":"ready","files":files,"truncated":truncated}),
    ))
}

#[derive(Deserialize)]
struct IssuesQuery {
    page: Option<u32>,
    category: Option<IssueCategory>,
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum IssueCategory {
    Bug,
    Vulnerability,
    CodeSmell,
}

impl IssueCategory {
    fn sonar_type(self) -> &'static str {
        match self {
            Self::Bug => "BUG",
            Self::Vulnerability => "VULNERABILITY",
            Self::CodeSmell => "CODE_SMELL",
        }
    }
}

async fn analysis_issues(
    user: AuthUser,
    State(state): State<SonarState>,
    Path((source, task)): Path<(String, String)>,
    Query(query): Query<IssuesQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    if !can_read_quality(&user) {
        return Err(StatusCode::FORBIDDEN);
    }
    let report = archived_analysis(&state, &source, &task).await?;
    let token = std::env::var("SHEPHERD_SONAR_DETAIL_TOKEN")
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let base =
        std::env::var("SHEPHERD_SONAR_DETAIL_URL").map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let base = base.trim_end_matches('/');
    let project = report["projectKey"].as_str().ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let revision = report["revision"].as_str().ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let expected_date = report["analyzedAt"]
        .as_str()
        .and_then(|date| chrono::DateTime::parse_from_rfc3339(date).ok());
    let latest_request = |branch: Option<&str>| {
        let request = state
            .client
            .get(format!("{base}/api/project_analyses/search"))
            .bearer_auth(&token)
            .query(&[("project", project), ("ps", "1")]);
        match branch {
            Some(branch) => request.query(&[("branch", branch)]),
            None => request,
        }
    };
    let matches_analysis = |result: &Value| {
        let latest = &result["analyses"][0];
        latest["revision"].as_str() == Some(revision)
            && expected_date.is_some()
            && latest["date"]
                .as_str()
                .and_then(|date| chrono::DateTime::parse_from_str(date, "%Y-%m-%dT%H:%M:%S%z").ok())
                == expected_date
    };
    let mut branch = None;
    if !matches_analysis(&upstream_json(latest_request(None)).await?) {
        branch = report["branch"].as_str().or_else(|| report["push"]["branch"].as_str()).filter(
            |branch| !project.contains(":branch:") && !matches!(*branch, "main" | "master"),
        );
        if branch.is_none()
            || !matches_analysis(
                &upstream_json(latest_request(branch)).await.unwrap_or(Value::Null),
            )
        {
            return Ok(no_store(json!({"status":"historical","items":[],"total":0})));
        }
    }
    let page = query.page.unwrap_or(1).clamp(1, 100);
    let request = state
        .client
        .get(format!("{base}/api/issues/search"))
        .bearer_auth(&token)
        .query(&[("components", project), ("ps", "25"), ("p", &page.to_string())]);
    let request = match query.category {
        Some(category) => request.query(&[("types", category.sonar_type())]),
        None => request,
    };
    let report = upstream_json(if let Some(branch) = branch {
        request.query(&[("branch", branch)])
    } else {
        request
    })
    .await?;
    if !matches_analysis(&upstream_json(latest_request(branch)).await?) {
        return Ok(no_store(json!({"status":"historical","items":[],"total":0})));
    }
    let prefix = format!("{project}:");
    let items: Vec<_> = report["issues"]
        .as_array()
        .ok_or(StatusCode::BAD_GATEWAY)?
        .iter()
        .map(|issue| {
            let component = issue["component"].as_str().unwrap_or_default();
            json!({"key":issue["key"],"rule":issue["rule"],"severity":issue["severity"],
            "type":issue["type"],"status":issue["issueStatus"],"message":issue["message"],
            "file":component.strip_prefix(&prefix).unwrap_or(component),"line":issue["line"]})
        })
        .collect();
    Ok(no_store(
        json!({"status":"ready","items":items,"total":report["paging"]["total"],"page":page}),
    ))
}

async fn list(
    user: AuthUser,
    State(state): State<SonarState>,
    Path(source): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    if !can_read_quality(&user) {
        return Err(StatusCode::FORBIDDEN);
    }
    sqlx::query_scalar(
        "SELECT jsonb_build_object('items',COALESCE(jsonb_agg(report ORDER BY analyzed_at DESC,task_id),'[]'))
           FROM (SELECT report,analyzed_at,task_id FROM ms_sonar_analysis WHERE source_id=$1
                 ORDER BY analyzed_at DESC,task_id LIMIT 100) recent",
    ).bind(source).fetch_one(&state.pool).await.map(Json).map_err(storage)
}

fn storage(error: sqlx::Error) -> StatusCode {
    tracing::error!(%error, "Sonar storage failed");
    StatusCode::INTERNAL_SERVER_ERROR
}

#[cfg(test)]
mod notification_tests {
    use super::*;

    #[test]
    fn validates_notification_destinations() {
        assert!(valid_notification_url("http://127.0.0.2:9000/sonar", false));
        assert!(!valid_notification_url("https://sonar.example.com/?id=old", false));
        assert!(valid_notification_url(
            "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=example",
            true
        ));
        assert!(!valid_notification_url(
            "https://qyapi.weixin.qq.com.example.com/cgi-bin/webhook/send?key=example",
            true
        ));
        assert!(!valid_notification_url("http://127.0.0.1/cgi-bin/webhook/send?key=example", true));
        assert!(!valid_notification_url(
            "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=one&key=two",
            true
        ));
    }

    #[test]
    fn validates_wecom_templates() {
        assert!(valid_notification_template(&json!({
            "msgtype": "template_card",
            "template_card": {
                "card_type": "text_notice",
                "main_title": {"title": "${project}"},
                "horizontal_content_list": "${rows}",
                "card_action": {"type": 1, "url": "${dashboard}"}
            }
        })));
        assert!(!valid_notification_template(&json!({
            "msgtype": "markdown",
            "markdown": {"content": "${unknown}"},
        })));
        assert!(!valid_notification_template(&json!({"msgtype": "text", "text": []})));
    }
}
