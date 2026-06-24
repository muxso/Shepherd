//! 组装根桥:接口定义的「URL 导入」与「定时导入」(tokio-cron-scheduler)。
//!
//! - URL 导入(POST /api/definition/import-url):服务端用 reqwest 拉取来源 URL,按格式解析并导入,
//!   绕开浏览器跨域;支持 Bearer/Basic token。
//! - 定时导入(/api/import-schedule):给一个 URL 来源配 cron,后台调度器到点拉取并导入;支持
//!   覆盖/不覆盖、按标签建模块、同步目录等(与一次性导入同一套选项)。
//! 启动时加载所有启用计划并注册;新建/启停立即生效,无需重启。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, post, put},
    Json, Router,
};
use migrate::PgPool;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_cron_scheduler::{Job, JobScheduler};
use webauth::{AuthUser, SessionStore};

use api_definition::adapters::pg::PgApiDefinitionRepository;
use api_definition::adapters::PgImportScheduleStore;
use api_definition::application::{
    CreateImportScheduleError, ImportApiDefinitionsUseCase, ImportError, ImportScheduleUseCase,
};
use api_definition::domain::{ImportFormat, ImportSchedule, NewImportSchedule};

#[derive(Clone)]
struct ImportSchedState {
    import_uc: ImportApiDefinitionsUseCase,
    schedules: ImportScheduleUseCase,
    client: reqwest::Client,
    sched: Arc<JobScheduler>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<ImportSchedState> for Arc<dyn SessionStore> {
    fn from_ref(s: &ImportSchedState) -> Self {
        s.sessions.clone()
    }
}

/// 拉取来源 URL → 解析为可导入文档(JMeter 为原始文本字符串,其余为 JSON)。
async fn fetch_doc(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    basic_auth: bool,
    format: ImportFormat,
) -> Result<Value, String> {
    let mut req = client.get(url.trim());
    if !token.trim().is_empty() {
        let scheme = if basic_auth { "Basic" } else { "Bearer" };
        req = req.header("Authorization", format!("{scheme} {}", token.trim()));
    }
    let resp = req.send().await.map_err(|e| format!("请求失败:{e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("来源返回 HTTP {}", status.as_u16()));
    }
    let text = resp.text().await.map_err(|e| format!("读取响应失败:{e}"))?;
    match format {
        // JMeter 为 XML:整段文本作为 JSON 字符串交给解析器。
        ImportFormat::Jmeter => Ok(Value::String(text)),
        _ => serde_json::from_str(&text).map_err(|e| format!("来源非法 JSON:{e}")),
    }
}

/// 跑一次定时导入:重载计划(取最新选项,容忍中途被停用/删除)→ 拉取 → 导入 → 回写结果摘要。
async fn run_schedule(
    import_uc: &ImportApiDefinitionsUseCase,
    schedules: &ImportScheduleUseCase,
    client: &reqwest::Client,
    id: &str,
) {
    let Some(s) = schedules.get(id).await.ok().flatten() else { return };
    if !s.enabled {
        return; // 已停用:跳过(live job 可能仍在,double-check)
    }
    let summary = run_import(import_uc, client, &s).await;
    // cron 自动触发:操作人为空串(前端展示为「自动」)。
    if let Err(e) = schedules.record_run(id, &summary, "").await {
        tracing::warn!(schedule = %id, "record import run failed: {e:?}");
    }
    tracing::info!(schedule = %id, %summary, "scheduled import done");
}

/// 按计划拉取并导入,返回结果摘要(成功/失败均为人类可读串)。
async fn run_import(
    import_uc: &ImportApiDefinitionsUseCase,
    client: &reqwest::Client,
    s: &ImportSchedule,
) -> String {
    let format = ImportFormat::from_str(&s.format);
    let doc = match fetch_doc(client, &s.source_url, &s.auth_token, s.basic_auth, format).await {
        Ok(d) => d,
        Err(e) => return format!("拉取失败:{e}"),
    };
    match import_uc
        .execute(
            &s.project_id,
            format,
            &doc,
            s.module_id.as_deref(),
            s.group_by_tag,
            s.overwrite,
            s.sync_module,
        )
        .await
    {
        Ok(o) => format!("新增 {} / 覆盖 {} / 跳过 {}", o.created.len(), o.updated, o.skipped),
        Err(ImportError::Parse(_)) => "导入失败:文档无法解析".to_string(),
        Err(ImportError::Repo(_)) => "导入失败:存储错误".to_string(),
    }
}

/// 注册一个 cron job:到点跑 [`run_schedule`]。尽力而为,出错只记日志。
async fn register_job(
    sched: &JobScheduler,
    import_uc: &ImportApiDefinitionsUseCase,
    schedules: &ImportScheduleUseCase,
    client: &reqwest::Client,
    id: &str,
    cron: &str,
) {
    let import_uc = import_uc.clone();
    let schedules = schedules.clone();
    let client = client.clone();
    let id_owned = id.to_string();
    let job = Job::new_async(cron, move |_uuid, _l| {
        let import_uc = import_uc.clone();
        let schedules = schedules.clone();
        let client = client.clone();
        let id = id_owned.clone();
        Box::pin(async move {
            run_schedule(&import_uc, &schedules, &client, &id).await;
        })
    });
    match job {
        Ok(j) => {
            if let Err(e) = sched.add(j).await {
                tracing::warn!(schedule = %id, "add import cron job failed: {e}");
            }
        }
        Err(e) => tracing::warn!(schedule = %id, cron, "invalid cron: {e}"),
    }
}

/// 构建路由:创建并启动 JobScheduler,加载已启用计划注册其 job,返回 axum 路由。
pub async fn build(
    pool: PgPool,
    sessions: Arc<dyn SessionStore>,
) -> Result<Router, Box<dyn std::error::Error>> {
    let apidef_repo = Arc::new(PgApiDefinitionRepository::new(pool.clone()));
    let import_uc = ImportApiDefinitionsUseCase::new(apidef_repo);
    let store = Arc::new(PgImportScheduleStore::new(pool.clone()));
    let schedules = ImportScheduleUseCase::new(store);
    let client = reqwest::Client::new();

    let sched = JobScheduler::new().await?;
    if let Ok(list) = schedules.list_enabled().await {
        for s in list {
            register_job(&sched, &import_uc, &schedules, &client, &s.id, &s.cron).await;
        }
    }
    sched.start().await?;
    let sched = Arc::new(sched);

    Ok(Router::new()
        .route("/api/definition/import-url", post(import_url))
        .route("/api/import-schedule", post(create_schedule).get(list_schedules))
        .route("/api/import-schedule/{id}", delete(delete_schedule))
        .route("/api/import-schedule/{id}/enabled", put(set_enabled))
        .route("/api/import-schedule/{id}/run", post(run_now))
        .with_state(ImportSchedState { import_uc, schedules, client, sched, sessions }))
}

// ---------- DTO ----------

/// URL 导入与定时导入共用的导入选项。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportUrlBody {
    project_id: String,
    #[serde(default)]
    format: Option<String>,
    url: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    basic_auth: bool,
    #[serde(default)]
    module_id: Option<String>,
    #[serde(default = "default_true")]
    group_by_tag: bool,
    #[serde(default = "default_true")]
    overwrite: bool,
    #[serde(default)]
    sync_module: bool,
}

fn default_true() -> bool {
    true
}

async fn import_url(
    user: AuthUser,
    State(st): State<ImportSchedState>,
    Json(b): Json<ImportUrlBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let format = ImportFormat::from_str(b.format.as_deref().unwrap_or("openapi"));
    let doc = match fetch_doc(&st.client, &b.url, b.token.as_deref().unwrap_or(""), b.basic_auth, format).await {
        Ok(d) => d,
        Err(e) => return (StatusCode::BAD_GATEWAY, e).into_response(),
    };
    match st
        .import_uc
        .execute(
            &b.project_id,
            format,
            &doc,
            b.module_id.as_deref().filter(|s| !s.is_empty()),
            b.group_by_tag,
            b.overwrite,
            b.sync_module,
        )
        .await
    {
        Ok(o) => (
            StatusCode::CREATED,
            Json(json!({ "created": o.created.len(), "updated": o.updated, "skipped": o.skipped })),
        )
            .into_response(),
        Err(ImportError::Parse(_)) => {
            (StatusCode::BAD_REQUEST, "来源文档无法解析").into_response()
        }
        Err(ImportError::Repo(_)) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateScheduleBody {
    project_id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    format: Option<String>,
    url: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    basic_auth: bool,
    #[serde(default)]
    module_id: Option<String>,
    #[serde(default = "default_true")]
    group_by_tag: bool,
    #[serde(default = "default_true")]
    overwrite: bool,
    #[serde(default)]
    sync_module: bool,
    cron: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleResponse {
    id: String,
    project_id: String,
    name: String,
    format: String,
    source_url: String,
    basic_auth: bool,
    module_id: Option<String>,
    group_by_tag: bool,
    overwrite: bool,
    sync_module: bool,
    cron: String,
    enabled: bool,
    last_run_at: Option<String>,
    last_result: String,
    last_run_by: String,
    created_by: String,
    created_at: String,
}

impl From<ImportSchedule> for ScheduleResponse {
    fn from(s: ImportSchedule) -> Self {
        // 注意:不回吐 auth_token(敏感)。
        Self {
            id: s.id,
            project_id: s.project_id,
            name: s.name,
            format: s.format,
            source_url: s.source_url,
            basic_auth: s.basic_auth,
            module_id: s.module_id,
            group_by_tag: s.group_by_tag,
            overwrite: s.overwrite,
            sync_module: s.sync_module,
            cron: s.cron,
            enabled: s.enabled,
            last_run_at: s.last_run_at,
            last_result: s.last_result,
            last_run_by: s.last_run_by,
            created_by: s.created_by,
            created_at: s.created_at,
        }
    }
}

async fn create_schedule(
    user: AuthUser,
    State(st): State<ImportSchedState>,
    Json(b): Json<CreateScheduleBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let new = NewImportSchedule {
        project_id: b.project_id,
        name: b.name.unwrap_or_default(),
        format: b.format.unwrap_or_else(|| "openapi".to_string()),
        source_url: b.url,
        auth_token: b.token.unwrap_or_default(),
        basic_auth: b.basic_auth,
        module_id: b.module_id,
        group_by_tag: b.group_by_tag,
        overwrite: b.overwrite,
        sync_module: b.sync_module,
        cron: b.cron,
        enabled: b.enabled,
        created_by: user.user_id.clone(),
    };
    match st.schedules.create(new).await {
        Ok(s) => {
            // 启用的计划立即挂上 live cron(无需重启)。
            if s.enabled {
                register_job(&st.sched, &st.import_uc, &st.schedules, &st.client, &s.id, &s.cron).await;
            }
            (StatusCode::CREATED, Json(ScheduleResponse::from(s))).into_response()
        }
        Err(CreateImportScheduleError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid schedule (project / url / cron)").into_response()
        }
        Err(CreateImportScheduleError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    project_id: String,
}

async fn list_schedules(State(st): State<ImportSchedState>, Query(q): Query<ListQuery>) -> Response {
    match st.schedules.list_by_project(&q.project_id).await {
        Ok(list) => {
            let items: Vec<ScheduleResponse> = list.into_iter().map(ScheduleResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

async fn delete_schedule(
    user: AuthUser,
    State(st): State<ImportSchedState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.schedules.delete(&id).await {
        // 注:live cron job 在下次重启前仍可能触发,但 run_schedule 重载时取不到(已软删)即跳过。
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnabledBody {
    enabled: bool,
}

async fn set_enabled(
    user: AuthUser,
    State(st): State<ImportSchedState>,
    Path(id): Path<String>,
    Json(b): Json<EnabledBody>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    if st.schedules.set_enabled(&id, b.enabled).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
    }
    // 启用:立即挂上 live cron(重复挂上仅多一个 job,run_schedule 会 double-check enabled)。
    if b.enabled {
        if let Ok(Some(s)) = st.schedules.get(&id).await {
            register_job(&st.sched, &st.import_uc, &st.schedules, &st.client, &s.id, &s.cron).await;
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn run_now(
    user: AuthUser,
    State(st): State<ImportSchedState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("API_DEFINITION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let Some(s) = st.schedules.get(&id).await.ok().flatten() else {
        return (StatusCode::NOT_FOUND, "schedule not found").into_response();
    };
    let summary = run_import(&st.import_uc, &st.client, &s).await;
    // 手动「立即执行」:记录触发的当前用户为操作人。
    let _ = st.schedules.record_run(&id, &summary, &user.user_id).await;
    (StatusCode::OK, Json(json!({ "result": summary }))).into_response()
}
