//! Host boundary: resolves identity/environment membership; provider data stays in its own database.
use async_trait::async_trait;
use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use environment_plugins::{
    domain::{Binding, BindingSlot, NewProfile, Profile, StoreError},
    pg::{DatabaseIdentity, PgProfileStore},
    ports::ProfileStore,
    vault::{CredentialVault, Keyring},
};
use plugin_runtime::*;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{collections::BTreeMap, sync::Arc};
use webauth::{AuthUser, SessionStore};

pub fn enabled() -> bool {
    std::env::var("SHEPHERD_PLUGINS")
        .unwrap_or_default()
        .split(',')
        .any(|id| id.trim() == "environment")
}

#[derive(Clone)]
pub struct Runtime {
    store: PgProfileStore,
    registry: PluginRegistry,
    keys: Arc<Keyring>,
}
impl Runtime {
    pub async fn load(host: &PgPool) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        if !enabled() {
            return Ok(None);
        }
        let url = std::env::var("SHEPHERD_ENVIRONMENT_PLUGINS_DATABASE_URL")
            .map_err(|_| "environment plugin requires its own database URL")?;
        let encoded = std::env::var("SHEPHERD_ENVIRONMENT_PLUGIN_KEYS")
            .map_err(|_| "environment plugin requires credential encryption keys")?;
        let raw: BTreeMap<String, String> =
            serde_json::from_str(&encoded).map_err(|_| "invalid environment plugin keyring")?;
        let keys = raw
            .into_iter()
            .map(|(id, key)| {
                let key: [u8; 32] = STANDARD
                    .decode(key)
                    .map_err(|_| "invalid credential encryption key")?
                    .try_into()
                    .map_err(|_| "credential encryption keys must be 32 bytes")?;
                Ok::<_, &str>((id, key))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let keys = Arc::new(Keyring::new(
            std::env::var("SHEPHERD_ENVIRONMENT_PLUGIN_ACTIVE_KEY").unwrap_or_else(|_| "v1".into()),
            keys,
        )?);
        let identity = DatabaseIdentity::read(host).await?;
        let store = PgProfileStore::connect(&url, &identity).await?;
        Ok(Some(Self { store, registry: environment_plugins::providers::registry()?, keys }))
    }
    fn vault(&self, policy: Arc<HostPolicy>) -> CredentialVault {
        CredentialVault::new(self.store.clone(), self.keys.clone(), policy)
    }
}

#[derive(Clone)]
struct HostPolicy {
    host: PgPool,
    user: AuthUser,
}
#[derive(Clone, Copy)]
enum Access {
    Read,
    Write,
    Execute,
}
impl HostPolicy {
    async fn check(&self, scope: &ExecutionScope, access: Access) -> Result<(), PluginError> {
        scope.validate()?;
        let read = self.user.can("ENVIRONMENT", "READ");
        let allowed = match access {
            Access::Read => read,
            Access::Write => read && self.user.can("ENVIRONMENT", "UPDATE"),
            Access::Execute => {
                read && ["API_DEFINITION", "API_SCENARIO", "TEST_PLAN", "RUNNER"]
                    .iter()
                    .any(|resource| self.user.can(resource, "EXECUTE"))
            }
        };
        if scope.actor_id != self.user.user_id || !allowed {
            return Err(PluginError::PermissionDenied);
        }
        let row=sqlx::query_as::<_,(bool,Option<String>)>("SELECT e.enabled,(SELECT role FROM ms_project_member m WHERE m.project_id=e.project_id AND m.user_id=$3) FROM ms_environment e JOIN ms_project p ON p.id=e.project_id WHERE e.id=$1 AND e.project_id=$2 AND NOT e.deleted AND NOT p.deleted")
            .bind(&scope.environment_id).bind(&scope.project_id).bind(&scope.actor_id).fetch_optional(&self.host).await
            .map_err(|_|PluginError::Unavailable("environment authorization".into()))?.ok_or(PluginError::PermissionDenied)?;
        let administrator = self.user.can("SYSTEM_USER", "UPDATE");
        let member = match access {
            Access::Read => matches!(row.1.as_deref(), Some("OWNER" | "MEMBER" | "VIEWER")),
            _ => matches!(row.1.as_deref(), Some("OWNER" | "MEMBER")),
        };
        if (!administrator && !member) || (matches!(access, Access::Execute) && !row.0) {
            return Err(PluginError::PermissionDenied);
        }
        Ok(())
    }
}
#[async_trait]
impl ExecutionAuthorizer for HostPolicy {
    async fn authorize(
        &self,
        scope: &ExecutionScope,
        plugin: &str,
        operation: &str,
    ) -> Result<(), PluginError> {
        let access = match (plugin, operation) {
            ("credentials", "list") => Access::Read,
            ("credentials", "create" | "delete") => Access::Write,
            _ => Access::Execute,
        };
        self.check(scope, access).await
    }
}

#[derive(Clone)]
struct AppState {
    host: PgPool,
    sessions: Arc<dyn SessionStore>,
    runtime: Option<Runtime>,
}
impl FromRef<AppState> for Arc<dyn SessionStore> {
    fn from_ref(state: &AppState) -> Self {
        state.sessions.clone()
    }
}
pub fn router(host: PgPool, sessions: Arc<dyn SessionStore>, runtime: Option<Runtime>) -> Router {
    Router::new()
        .route("/api/environment-plugins/capabilities", get(capabilities))
        .route("/api/environment-plugins/environments/{environment}", get(configuration))
        .route("/api/environment-plugins/environments/{environment}/profiles", post(create_profile))
        .route(
            "/api/environment-plugins/environments/{environment}/profiles/{profile}",
            put(update_profile).delete(delete_profile),
        )
        .route(
            "/api/environment-plugins/environments/{environment}/bindings/{profile}",
            put(bind_profile).delete(unbind_profile),
        )
        .route(
            "/api/environment-plugins/environments/{environment}/profiles/{profile}/test",
            post(test_profile),
        )
        .route(
            "/api/environment-plugins/environments/{environment}/profiles/{profile}/execute",
            post(execute_profile),
        )
        .route(
            "/api/environment-plugins/environments/{environment}/credentials",
            post(create_credential),
        )
        .route(
            "/api/environment-plugins/environments/{environment}/credentials/{reference}",
            axum::routing::delete(delete_credential),
        )
        .with_state(AppState { host, sessions, runtime })
}

enum ApiError {
    Plugin(PluginError),
    Store(StoreError),
    NotFound,
}
impl From<PluginError> for ApiError {
    fn from(error: PluginError) -> Self {
        Self::Plugin(error)
    }
}
impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Plugin(e) => {
                let status = match e {
                    PluginError::PermissionDenied => StatusCode::FORBIDDEN,
                    PluginError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
                    PluginError::Timeout => StatusCode::GATEWAY_TIMEOUT,
                    PluginError::Cancelled => StatusCode::REQUEST_TIMEOUT,
                    PluginError::Execution(_) => StatusCode::BAD_GATEWAY,
                    _ => StatusCode::BAD_REQUEST,
                };
                (status, e.to_string())
            }
            Self::Store(e) => {
                let status = match e {
                    StoreError::NotFound => StatusCode::NOT_FOUND,
                    StoreError::Conflict => StatusCode::CONFLICT,
                    StoreError::Unavailable | StoreError::HostDatabase => {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                    StoreError::Invalid => StatusCode::BAD_REQUEST,
                };
                (status, e.to_string())
            }
            Self::NotFound => (StatusCode::NOT_FOUND, "environment not found".into()),
        };
        (status, Json(json!({"error":message}))).into_response()
    }
}
type ApiResult = Result<Json<Value>, ApiError>;

impl AppState {
    async fn scope(
        &self,
        user: AuthUser,
        environment: &str,
        access: Access,
    ) -> Result<(ExecutionScope, Arc<HostPolicy>, Runtime), ApiError> {
        if !user.can("ENVIRONMENT", "READ") {
            return Err(PluginError::PermissionDenied.into());
        }
        let project = sqlx::query_scalar::<_, String>(
            "SELECT project_id FROM ms_environment WHERE id=$1 AND NOT deleted",
        )
        .bind(environment)
        .fetch_optional(&self.host)
        .await
        .map_err(|_| PluginError::Unavailable("environment authorization".into()))?
        .ok_or(ApiError::NotFound)?;
        let scope = ExecutionScope {
            project_id: project,
            environment_id: environment.into(),
            actor_id: user.user_id.clone(),
        };
        let policy = Arc::new(HostPolicy { host: self.host.clone(), user });
        policy.check(&scope, access).await?;
        let runtime =
            self.runtime.clone().ok_or_else(|| PluginError::Unavailable("environment".into()))?;
        Ok((scope, policy, runtime))
    }
}
#[utoipa::path(get,path="/api/environment-plugins/capabilities",tag="environment-plugins",responses((status=200,body=Value),(status=400),(status=401),(status=403),(status=404),(status=409),(status=503)),security(("bearer"=[])))]
async fn capabilities(user: AuthUser, State(state): State<AppState>) -> ApiResult {
    if !user.can("ENVIRONMENT", "READ") {
        return Err(PluginError::PermissionDenied.into());
    }
    Ok(Json(
        json!({"installed":true,"enabled":state.runtime.is_some(),"items":state.runtime.as_ref().map(|r|r.registry.descriptors()).unwrap_or_default()}),
    ))
}
#[utoipa::path(get,path="/api/environment-plugins/environments/{environment}",tag="environment-plugins",params(("environment"=String,Path)),responses((status=200,body=Value),(status=400),(status=401),(status=403),(status=404),(status=409),(status=503)),security(("bearer"=[])))]
async fn configuration(
    user: AuthUser,
    State(state): State<AppState>,
    Path(environment): Path<String>,
) -> ApiResult {
    let (scope, policy, runtime) = state.scope(user, &environment, Access::Read).await?;
    let manage = policy.check(&scope, Access::Write).await.is_ok();
    let execute = policy.check(&scope, Access::Execute).await.is_ok();
    Ok(Json(
        json!({"profiles":runtime.store.list(&scope.project_id).await?,"bindings":runtime.store.bindings(&scope.project_id,&environment).await?,"credentials":runtime.vault(policy).list(&scope).await?,"permissions":{"manage":manage,"execute":execute}}),
    ))
}
async fn validate_profile(
    runtime: &Runtime,
    scope: &ExecutionScope,
    policy: Arc<HostPolicy>,
    profile: &NewProfile,
) -> Result<(), ApiError> {
    profile.validate(&scope.project_id)?;
    runtime.registry.validate_config(&profile.plugin_id, &profile.config)?;
    let available = runtime.vault(policy).list(scope).await?;
    if profile.config.secrets.values().any(|r| !available.iter().any(|m| m.id == r.0)) {
        return Err(PluginError::PermissionDenied.into());
    }
    Ok(())
}
#[utoipa::path(post,path="/api/environment-plugins/environments/{environment}/profiles",tag="environment-plugins",params(("environment"=String,Path)),request_body=Value,responses((status=200,body=Value),(status=400),(status=422),(status=401),(status=403),(status=404),(status=409),(status=503)),security(("bearer"=[])))]
async fn create_profile(
    user: AuthUser,
    State(state): State<AppState>,
    Path(environment): Path<String>,
    Json(profile): Json<NewProfile>,
) -> ApiResult {
    let (scope, policy, runtime) = state.scope(user, &environment, Access::Write).await?;
    validate_profile(&runtime, &scope, policy, &profile).await?;
    Ok(Json(json!(runtime.store.create(&scope.project_id, profile).await?)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateProfile {
    revision: i64,
    profile: NewProfile,
}
#[utoipa::path(put,path="/api/environment-plugins/environments/{environment}/profiles/{profile}",tag="environment-plugins",params(("environment"=String,Path),("profile"=String,Path)),request_body=Value,responses((status=200,body=Value),(status=400),(status=422),(status=401),(status=403),(status=404),(status=409),(status=503)),security(("bearer"=[])))]
async fn update_profile(
    user: AuthUser,
    State(state): State<AppState>,
    Path((environment, id)): Path<(String, String)>,
    Json(request): Json<UpdateProfile>,
) -> ApiResult {
    let (scope, policy, runtime) = state.scope(user, &environment, Access::Write).await?;
    validate_profile(&runtime, &scope, policy, &request.profile).await?;
    Ok(Json(json!(
        runtime.store.update(&scope.project_id, &id, request.revision, request.profile).await?
    )))
}
#[derive(Deserialize)]
struct Revision {
    revision: i64,
}
#[utoipa::path(delete,path="/api/environment-plugins/environments/{environment}/profiles/{profile}",tag="environment-plugins",params(("environment"=String,Path),("profile"=String,Path)),responses((status=200,body=Value),(status=400),(status=401),(status=403),(status=404),(status=409),(status=503)),security(("bearer"=[])))]
async fn delete_profile(
    user: AuthUser,
    State(state): State<AppState>,
    Path((environment, id)): Path<(String, String)>,
    Query(request): Query<Revision>,
) -> ApiResult {
    let (scope, _, runtime) = state.scope(user, &environment, Access::Write).await?;
    runtime.store.delete(&scope.project_id, &id, request.revision).await?;
    Ok(Json(json!({"deleted":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindRequest {
    slot: BindingSlot,
    priority: i32,
    enabled: bool,
}
fn supports(profile: &Profile, runtime: &Runtime, slot: BindingSlot) -> Result<(), PluginError> {
    let descriptor = runtime
        .registry
        .descriptors()
        .into_iter()
        .find(|d| d.id == profile.plugin_id)
        .ok_or_else(|| PluginError::Unavailable(profile.plugin_id.clone()))?;
    let required = match slot {
        BindingSlot::Connection | BindingSlot::Cleanup => Capability::Execute,
        BindingSlot::Pre => Capability::Preprocess,
        BindingSlot::Post => Capability::Postprocess,
        BindingSlot::Assert => Capability::Assert,
        BindingSlot::Host => Capability::HostOverride,
        BindingSlot::Display => Capability::Display,
    };
    if !descriptor.capabilities.contains(&required) {
        return Err(PluginError::InvalidConfig("plugin cannot be used in this slot".into()));
    }
    Ok(())
}
#[utoipa::path(put,path="/api/environment-plugins/environments/{environment}/bindings/{profile}",tag="environment-plugins",params(("environment"=String,Path),("profile"=String,Path)),request_body=Value,responses((status=200,body=Value),(status=400),(status=422),(status=401),(status=403),(status=404),(status=409),(status=503)),security(("bearer"=[])))]
async fn bind_profile(
    user: AuthUser,
    State(state): State<AppState>,
    Path((environment, id)): Path<(String, String)>,
    Json(request): Json<BindRequest>,
) -> ApiResult {
    let (scope, policy, runtime) = state.scope(user, &environment, Access::Write).await?;
    let profile = runtime.store.get(&scope.project_id, &id).await?;
    supports(&profile, &runtime, request.slot)?;
    validate_profile(
        &runtime,
        &scope,
        policy,
        &NewProfile {
            plugin_id: profile.plugin_id.clone(),
            name: profile.name.clone(),
            config: profile.config.clone(),
            enabled: profile.enabled,
        },
    )
    .await?;
    runtime
        .store
        .bind(Binding {
            project_id: scope.project_id,
            environment_id: environment,
            profile_id: id,
            slot: request.slot,
            priority: request.priority,
            enabled: request.enabled,
        })
        .await?;
    Ok(Json(json!({"bound":true})))
}
#[derive(Deserialize)]
struct Slot {
    slot: BindingSlot,
}
#[utoipa::path(delete,path="/api/environment-plugins/environments/{environment}/bindings/{profile}",tag="environment-plugins",params(("environment"=String,Path),("profile"=String,Path)),responses((status=200,body=Value),(status=400),(status=401),(status=403),(status=404),(status=409),(status=503)),security(("bearer"=[])))]
async fn unbind_profile(
    user: AuthUser,
    State(state): State<AppState>,
    Path((environment, id)): Path<(String, String)>,
    Query(request): Query<Slot>,
) -> ApiResult {
    let (scope, _, runtime) = state.scope(user, &environment, Access::Write).await?;
    runtime.store.unbind(&scope.project_id, &environment, request.slot, &id).await?;
    Ok(Json(json!({"unbound":true})))
}
async fn invoke(
    state: &AppState,
    user: AuthUser,
    environment: String,
    id: String,
    operation: &str,
    input: Value,
) -> ApiResult {
    let (scope, policy, runtime) = state.scope(user, &environment, Access::Execute).await?;
    let profile = runtime.store.get(&scope.project_id, &id).await?;
    if !profile.enabled {
        return Err(PluginError::Unavailable(profile.plugin_id).into());
    }
    if operation != "test"
        && !runtime
            .store
            .bindings(&scope.project_id, &environment)
            .await?
            .iter()
            .any(|b| b.profile_id == id && b.enabled)
    {
        return Err(PluginError::PermissionDenied.into());
    }
    let context = ExecutionContext {
        scope,
        limits: ExecutionLimits::default(),
        secrets: Arc::new(runtime.vault(policy.clone())),
        authorization: policy,
        cancellation: Arc::new(CancellationToken::default()),
    };
    let output = runtime
        .registry
        .execute(
            &Invocation {
                plugin_id: profile.plugin_id,
                operation: operation.into(),
                config: profile.config,
                input,
            },
            &context,
        )
        .await?;
    Ok(Json(json!(output)))
}
#[utoipa::path(post,path="/api/environment-plugins/environments/{environment}/profiles/{profile}/test",tag="environment-plugins",params(("environment"=String,Path),("profile"=String,Path)),responses((status=200,body=Value),(status=400),(status=401),(status=403),(status=404),(status=409),(status=503)),security(("bearer"=[])))]
async fn test_profile(
    user: AuthUser,
    State(state): State<AppState>,
    Path((environment, id)): Path<(String, String)>,
) -> ApiResult {
    invoke(&state, user, environment, id, "test", Value::Null).await
}
#[utoipa::path(post,path="/api/environment-plugins/environments/{environment}/profiles/{profile}/execute",tag="environment-plugins",params(("environment"=String,Path),("profile"=String,Path)),request_body=Value,responses((status=200,body=Value),(status=400),(status=422),(status=401),(status=403),(status=404),(status=409),(status=503)),security(("bearer"=[])))]
async fn execute_profile(
    user: AuthUser,
    State(state): State<AppState>,
    Path((environment, id)): Path<(String, String)>,
    Json(input): Json<Value>,
) -> ApiResult {
    invoke(&state, user, environment, id, "execute", input).await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NewCredential {
    label: String,
    value: String,
    environment_only: bool,
}
#[utoipa::path(post,path="/api/environment-plugins/environments/{environment}/credentials",tag="environment-plugins",params(("environment"=String,Path)),request_body=Value,responses((status=200,body=Value),(status=400),(status=422),(status=401),(status=403),(status=404),(status=409),(status=503)),security(("bearer"=[])))]
async fn create_credential(
    user: AuthUser,
    State(state): State<AppState>,
    Path(environment): Path<String>,
    Json(request): Json<NewCredential>,
) -> ApiResult {
    let (scope, policy, runtime) = state.scope(user, &environment, Access::Write).await?;
    let reference = runtime
        .vault(policy)
        .create(&scope, &request.label, request.environment_only, SecretValue::new(request.value))
        .await?;
    Ok(Json(json!({"reference":reference})))
}
#[utoipa::path(delete,path="/api/environment-plugins/environments/{environment}/credentials/{reference}",tag="environment-plugins",params(("environment"=String,Path),("reference"=String,Path)),responses((status=200,body=Value),(status=400),(status=401),(status=403),(status=404),(status=409),(status=503)),security(("bearer"=[])))]
async fn delete_credential(
    user: AuthUser,
    State(state): State<AppState>,
    Path((environment, reference)): Path<(String, String)>,
) -> ApiResult {
    let (scope, policy, runtime) = state.scope(user, &environment, Access::Write).await?;
    runtime.vault(policy).delete(&scope, &SecretRef(reference)).await?;
    Ok(Json(json!({"deleted":true})))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(capabilities,configuration,create_profile,update_profile,delete_profile,bind_profile,unbind_profile,test_profile,execute_profile,create_credential,delete_credential),tags((name="environment-plugins",description="Plugin-owned environment configuration and execution")))]
struct Api;
pub fn openapi() -> utoipa::openapi::OpenApi {
    <Api as utoipa::OpenApi>::openapi()
}

#[cfg(test)]
#[path = "../../probe/tests/support/ssh.rs"]
mod ssh_fixture;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    use webauth::{testing::InMemorySessionStore, PermissionSet};

    async fn call(
        app: &Router,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
        status: u16,
    ) -> Value {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let body = if let Some(body) = body {
            request = request.header("Content-Type", "application/json");
            Body::from(body.to_string())
        } else {
            Body::empty()
        };
        let response =
            app.clone().oneshot(request.body(body).expect("request")).await.expect("response");
        assert_eq!(response.status().as_u16(), status, "{method} {path}");
        let bytes = to_bytes(response.into_body(), 2_000_000).await.expect("body");
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }
    async fn exercise_ssh_profiles(app: &Router, base: &str, owner: &str, viewer: &str) {
        let fixture = ssh_fixture::Fixture::start().await;
        let key=call(app,"POST",&format!("{base}/credentials"),Some(owner),Some(json!({"label":"SSH key fixture","value":fixture.encrypted_key,"environmentOnly":true})),200).await;
        let phrase=call(app,"POST",&format!("{base}/credentials"),Some(owner),Some(json!({"label":"SSH passphrase fixture","value":"fixture passphrase","environmentOnly":true})),200).await;
        let body = json!({"pluginId":"ssh","name":"SSH fixture","enabled":true,"config":{"schemaVersion":1,"values":{"host":fixture.target.host,"port":fixture.target.port,"username":fixture.target.username,"host_key_sha256":fixture.target.host_key_sha256},"secrets":{"private_key":key["reference"],"passphrase":phrase["reference"]}}});
        let mut plaintext = body.clone();
        plaintext["config"]["values"]["private_key"] = json!(fixture.private_key);
        call(app, "POST", &format!("{base}/profiles"), Some(owner), Some(plaintext), 400).await;
        let profile =
            call(app, "POST", &format!("{base}/profiles"), Some(owner), Some(body), 200).await;
        let id = profile["id"].as_str().expect("SSH profile id");
        let path = format!("{base}/profiles/{id}");
        call(app, "POST", &format!("{path}/test"), Some(owner), None, 200).await;
        call(
            app,
            "PUT",
            &format!("{base}/bindings/{id}"),
            Some(owner),
            Some(json!({"slot":"connection","priority":0,"enabled":true})),
            200,
        )
        .await;
        let result = call(
            app,
            "POST",
            &format!("{path}/execute"),
            Some(owner),
            Some(json!({"command":"echo fixture passphrase"})),
            200,
        )
        .await;
        assert_eq!(result["status"], "success");
        assert!(!result.to_string().contains("fixture passphrase"));
        let failed = call(
            app,
            "POST",
            &format!("{path}/execute"),
            Some(owner),
            Some(json!({"command":"streams"})),
            200,
        )
        .await;
        assert_eq!(failed["status"], "failure");
        assert_eq!(failed["data"]["termination"]["code"], 7);
        call(
            app,
            "POST",
            &format!("{path}/execute"),
            Some(viewer),
            Some(json!({"command":"echo denied"})),
            403,
        )
        .await;
        let loaded = call(app, "GET", base, Some(owner), None, 200).await;
        let encoded = loaded.to_string();
        assert!(!encoded.contains("BEGIN OPENSSH PRIVATE KEY"));
        assert!(!encoded.contains("fixture passphrase"));
        fixture.idle().await;
    }
    #[tokio::test]
    async fn disabled_capabilities_remain_authenticated_without_opening_a_plugin_database() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/unused")
            .expect("lazy pool");
        let sessions = Arc::new(InMemorySessionStore::new());
        let denied = sessions
            .create("denied", PermissionSet::from_raw(["PROJECT:READ"]).expect("permissions"), 3600)
            .await
            .expect("session");
        let reader = sessions
            .create(
                "reader",
                PermissionSet::from_raw(["ENVIRONMENT:READ"]).expect("permissions"),
                3600,
            )
            .await
            .expect("session");
        let app = router(pool, sessions, None);
        call(&app, "GET", "/api/environment-plugins/capabilities", None, None, 401).await;
        call(&app, "GET", "/api/environment-plugins/capabilities", Some(&denied), None, 403).await;
        let result =
            call(&app, "GET", "/api/environment-plugins/capabilities", Some(&reader), None, 200)
                .await;
        assert_eq!(result, json!({"installed":true,"enabled":false,"items":[]}));
        call(
            &app,
            "GET",
            "/api/environment-plugins/environments/unknown",
            Some(&denied),
            None,
            403,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires disposable ENVIRONMENT_PLUGIN_TEST_HOST_URL, ENVIRONMENT_PLUGIN_TEST_DATABASE_URL, and REDIS_URL"]
    async fn isolated_environment_plugin_api_persists_binds_and_tests_a_real_connection() {
        let host_url =
            std::env::var("ENVIRONMENT_PLUGIN_TEST_HOST_URL").expect("disposable host database");
        let plugin_url = std::env::var("ENVIRONMENT_PLUGIN_TEST_DATABASE_URL")
            .expect("disposable plugin database");
        let host = PgPool::connect(&host_url).await.expect("host");
        sqlx::raw_sql("CREATE TABLE ms_project(id TEXT PRIMARY KEY,deleted BOOL NOT NULL DEFAULT false); CREATE TABLE ms_environment(id TEXT PRIMARY KEY,project_id TEXT NOT NULL,enabled BOOL NOT NULL DEFAULT true,deleted BOOL NOT NULL DEFAULT false); CREATE TABLE ms_project_member(project_id TEXT,user_id TEXT,role TEXT,PRIMARY KEY(project_id,user_id)); INSERT INTO ms_project(id) VALUES('p'),('foreign'); INSERT INTO ms_environment(id,project_id) VALUES('e','p'),('other','foreign'); INSERT INTO ms_project_member VALUES('p','owner','OWNER'),('p','viewer','VIEWER');")
            .execute(&host).await.expect("host fixtures");
        let store = PgProfileStore::connect(
            &plugin_url,
            &DatabaseIdentity::read(&host).await.expect("identity"),
        )
        .await
        .expect("isolated store");
        let runtime = Runtime {
            store,
            registry: environment_plugins::providers::registry().expect("providers"),
            keys: Arc::new(
                Keyring::new("v1".into(), [("v1".into(), [31; 32])].into()).expect("keyring"),
            ),
        };
        let sessions = Arc::new(InMemorySessionStore::new());
        let permissions =
            PermissionSet::from_raw(["ENVIRONMENT:READ+UPDATE", "API_DEFINITION:EXECUTE"])
                .expect("permissions");
        let owner =
            sessions.create("owner", permissions.clone(), 3600).await.expect("owner session");
        let viewer =
            sessions.create("viewer", permissions.clone(), 3600).await.expect("viewer session");
        let stranger =
            sessions.create("stranger", permissions, 3600).await.expect("stranger session");
        let administrator = sessions
            .create(
                "administrator",
                PermissionSet::from_raw([
                    "ENVIRONMENT:READ+UPDATE",
                    "API_DEFINITION:EXECUTE",
                    "SYSTEM_USER:UPDATE",
                ])
                .expect("administrator permissions"),
                3600,
            )
            .await
            .expect("administrator session");
        let app = router(host.clone(), sessions, Some(runtime));
        let base = "/api/environment-plugins/environments/e";
        let capabilities =
            call(&app, "GET", "/api/environment-plugins/capabilities", Some(&owner), None, 200)
                .await;
        assert_eq!(capabilities["enabled"], true);
        assert!(capabilities["items"]
            .as_array()
            .expect("capabilities")
            .iter()
            .any(|p| p["id"] == "postgres"));
        call(&app, "GET", base, Some(&stranger), None, 403).await;
        call(&app, "GET", "/api/environment-plugins/environments/other", Some(&owner), None, 403)
            .await;
        let target = reqwest::Url::parse(&plugin_url).expect("target URL");
        let credential=call(&app,"POST",&format!("{base}/credentials"),Some(&owner),Some(json!({"label":"database password","value":target.password().expect("fixture password"),"environmentOnly":true})),200).await;
        let mut profile = json!({"pluginId":"postgres","name":"Local test database","enabled":true,"config":{"schemaVersion":1,"values":{"host":target.host_str().expect("host"),"port":target.port().unwrap_or(5432),"database":target.path().trim_start_matches('/'),"user":target.username(),"tls":"disable"},"secrets":{"password":credential["reference"]}}});
        call(&app, "POST", &format!("{base}/profiles"), Some(&viewer), Some(profile.clone()), 403)
            .await;
        let mut bad = profile.clone();
        bad["projectId"] = json!("foreign");
        call(&app, "POST", &format!("{base}/profiles"), Some(&owner), Some(bad), 422).await;
        let foreign_credential=call(&app,"POST","/api/environment-plugins/environments/other/credentials",Some(&administrator),Some(json!({"label":"foreign credential","value":"do-not-expose","environmentOnly":true})),200).await;
        let mut bad = profile.clone();
        bad["config"]["secrets"]["password"] = foreign_credential["reference"].clone();
        call(&app, "POST", &format!("{base}/profiles"), Some(&owner), Some(bad), 403).await;
        let created = call(
            &app,
            "POST",
            &format!("{base}/profiles"),
            Some(&owner),
            Some(profile.clone()),
            200,
        )
        .await;
        assert!(!created.to_string().contains(target.password().expect("password")));
        let id = created["id"].as_str().expect("profile id");
        let path = format!("{base}/profiles/{id}");
        let connection = call(&app, "POST", &format!("{path}/test"), Some(&owner), None, 200).await;
        assert_eq!(connection["status"], "success");
        call(&app, "POST", &format!("{path}/test"), Some(&viewer), None, 403).await;
        call(
            &app,
            "POST",
            &format!("{path}/execute"),
            Some(&owner),
            Some(json!({"query":"SELECT 1"})),
            403,
        )
        .await;
        call(
            &app,
            "PUT",
            &format!("{base}/bindings/{id}"),
            Some(&owner),
            Some(json!({"slot":"post","priority":0,"enabled":true})),
            400,
        )
        .await;
        call(
            &app,
            "PUT",
            &format!("{base}/bindings/{id}"),
            Some(&owner),
            Some(json!({"slot":"connection","priority":0,"enabled":true})),
            200,
        )
        .await;
        let result = call(
            &app,
            "POST",
            &format!("{path}/execute"),
            Some(&owner),
            Some(json!({"query":"SELECT 1"})),
            200,
        )
        .await;
        assert_eq!(result["status"], "success");
        assert_eq!(result["data"]["rows"][0][0], 1);
        let query = call(&app,"POST",&format!("{path}/execute"),Some(&owner),Some(json!({"query":"SELECT $1::text AS credential, 9007199254740993::bigint AS id", "parameters":[{"type":"text","value":target.password().expect("password")}],"extract":[{"variable":"secret","row":0,"column":"credential"},{"variable":"id","row":0,"column":1}]})),200).await;
        assert_eq!(query["status"], "success");
        assert_eq!(query["variables"]["id"], "9007199254740993");
        assert!(
            !query.to_string().contains(target.password().expect("password")),
            "query cells and variables must redact configured credentials"
        );
        let missing = call(&app,"POST",&format!("{path}/execute"),Some(&owner),Some(json!({"query":"SELECT 1 AS id WHERE false","extract":[{"variable":"id","row":0,"column":"id"}]})),200).await;
        assert_eq!(missing["status"], "failure");
        assert_eq!(missing["assertions"][0]["passed"], false);
        call(
            &app,
            "POST",
            &format!("{path}/execute"),
            Some(&viewer),
            Some(json!({"query":"SELECT 1"})),
            403,
        )
        .await;
        call(
            &app,
            "POST",
            &format!("{path}/execute"),
            Some(&owner),
            Some(json!({"query":"SELECT 1","ignored":true})),
            400,
        )
        .await;
        let loaded = call(&app, "GET", base, Some(&owner), None, 200).await;
        assert_eq!(loaded["profiles"].as_array().expect("profiles").len(), 1);
        assert_eq!(loaded["bindings"][0]["profileId"], id);
        assert_eq!(loaded["permissions"], json!({"manage":true,"execute":true}));
        let read_only = call(&app, "GET", base, Some(&viewer), None, 200).await;
        assert_eq!(read_only["permissions"], json!({"manage":false,"execute":false}));
        profile["name"] = json!("Updated name");
        call(&app, "PUT", &path, Some(&owner), Some(json!({"revision":1,"profile":profile})), 200)
            .await;
        call(&app, "PUT", &path, Some(&owner), Some(json!({"revision":1,"profile":profile})), 409)
            .await;
        call(&app, "DELETE", &format!("{path}?revision=2"), Some(&owner), None, 409).await;
        call(
            &app,
            "DELETE",
            &format!("{base}/bindings/{id}?slot=connection"),
            Some(&owner),
            None,
            200,
        )
        .await;
        call(&app, "DELETE", &format!("{path}?revision=2"), Some(&owner), None, 200).await;
        let redis_url = std::env::var("REDIS_URL").expect("disposable Redis URL");
        let redis_target = reqwest::Url::parse(&redis_url).expect("Redis URL");
        let redis_driver = probe::plugins::RedisPlugin::new();
        let redis_budget =
            probe::plugins::redis_command::RedisBudget { timeout_ms: 5_000, max_bytes: 65_536 };
        let redis_secret = "api fixture credential @:/?#";
        let redis_command = |name: &str, args: Value| {
            serde_json::from_value::<probe::domain::redis::RedisCommand>(
                json!({"command":name,"arguments":args}),
            )
            .expect("Redis command")
        };
        redis_driver
            .execute_command(
                &redis_url,
                &redis_command(
                    "ACL",
                    json!([
                        "SETUSER",
                        "ep_api_test",
                        "on",
                        format!(">{redis_secret}"),
                        "~ep_api:*",
                        "+@all"
                    ]),
                ),
                redis_budget,
            )
            .await
            .expect("Redis ACL fixture");
        let credential = call(
            &app,
            "POST",
            &format!("{base}/credentials"),
            Some(&owner),
            Some(json!({"label":"Redis password","value":redis_secret,"environmentOnly":true})),
            200,
        )
        .await;
        let body = json!({"pluginId":"redis","name":"Redis fixture","enabled":true,"config":{"schemaVersion":1,"values":{"host":redis_target.host_str().expect("host"),"port":redis_target.port().unwrap_or(6379),"database":13,"username":"ep_api_test","tls":if redis_target.scheme()=="rediss" {"require"} else {"disable"},"protocol":"resp3"},"secrets":{"password":credential["reference"]}}});
        let profile =
            call(&app, "POST", &format!("{base}/profiles"), Some(&owner), Some(body), 200).await;
        let redis_id = profile["id"].as_str().expect("Redis profile id");
        let redis_path = format!("{base}/profiles/{redis_id}");
        call(&app, "POST", &format!("{redis_path}/test"), Some(&owner), None, 200).await;
        call(
            &app,
            "PUT",
            &format!("{base}/bindings/{redis_id}"),
            Some(&owner),
            Some(json!({"slot":"connection","priority":0,"enabled":true})),
            200,
        )
        .await;
        let result=call(&app,"POST",&format!("{redis_path}/execute"),Some(&owner),Some(json!({"command":"ECHO","arguments":[redis_secret],"extract":[{"variable":"secret","pointer":""}]})),200).await;
        assert_eq!(result["status"], "success");
        assert!(
            !result.to_string().contains(redis_secret),
            "Redis replies and variables must redact configured credentials"
        );
        let encoded = redis_secret.bytes().map(|byte| format!("{byte:02x}")).collect::<String>();
        let binary=call(&app,"POST",&format!("{redis_path}/execute"),Some(&owner),Some(json!({"command":"ECHO","arguments":[{"hex":format!("ff{encoded}")}],"extract":[{"variable":"binary","pointer":"/hex"}]})),200).await;
        assert!(
            !binary.to_string().contains(&encoded),
            "binary Redis replies must redact hex-encoded credentials"
        );
        let missing=call(&app,"POST",&format!("{redis_path}/execute"),Some(&owner),Some(json!({"command":"ECHO","arguments":["hello world"],"extract":[{"variable":"missing","pointer":"/absent"}]})),200).await;
        assert_eq!(missing["status"], "failure");
        call(
            &app,
            "POST",
            &format!("{redis_path}/execute"),
            Some(&viewer),
            Some(json!({"command":"PING"})),
            403,
        )
        .await;
        call(
            &app,
            "POST",
            &format!("{redis_path}/execute"),
            Some(&owner),
            Some(json!({"command":"SELECT","arguments":["0"]})),
            400,
        )
        .await;
        redis_driver
            .execute_command(
                &redis_url,
                &redis_command("ACL", json!(["DELUSER", "ep_api_test"])),
                redis_budget,
            )
            .await
            .expect("Redis ACL cleanup");
        exercise_ssh_profiles(&app, base, &owner, &viewer).await;
        let projects: i64 = sqlx::query_scalar("SELECT count(*) FROM ms_project")
            .fetch_one(&host)
            .await
            .expect("host projects");
        let environments: i64 = sqlx::query_scalar("SELECT count(*) FROM ms_environment")
            .fetch_one(&host)
            .await
            .expect("host environments");
        let profiles: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_tables WHERE schemaname='public' AND tablename LIKE 'ep_%'",
        )
        .fetch_one(&host)
        .await
        .expect("host table isolation");
        assert_eq!((projects, environments, profiles), (2, 2, 0));
    }
}
