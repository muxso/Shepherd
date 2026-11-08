use crate::domain::test_assets::*;
use crate::ports::test_assets::{AssetSource, AssetStore};
use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use webauth::{AuthUser, SessionStore};
#[derive(Clone)]
struct AssetState {
    store: Arc<dyn AssetStore>,
    source: Option<Arc<dyn AssetSource>>,
    sessions: Arc<dyn SessionStore>,
}
impl FromRef<AssetState> for Arc<dyn SessionStore> {
    fn from_ref(s: &AssetState) -> Self {
        s.sessions.clone()
    }
}
pub fn router(
    store: Arc<dyn AssetStore>,
    source: Option<Arc<dyn AssetSource>>,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/test-assets/projects/{project}", get(overview))
        .route("/test-assets/projects/{project}/sources", get(sources))
        .route("/test-assets/projects/{project}/services", post(create_service))
        .route(
            "/test-assets/projects/{project}/services/{id}",
            put(update_service).delete(delete_service),
        )
        .route("/test-assets/projects/{project}/services/{id}/inventories", post(save_inventory))
        .route("/test-assets/projects/{project}/case-services", put(assign))
        .route("/test-assets/projects/{project}/cases/{id}/api-links", put(set_case_api))
        .route("/test-assets/projects/{project}/mappings", put(mapping))
        .route("/test-assets/projects/{project}/imports", post(preview))
        .route("/test-assets/projects/{project}/imports/{id}/items", get(import_item))
        .route("/test-assets/projects/{project}/imports/{id}/apply", post(apply))
        .route("/test-assets/projects/{project}/scenarios", post(create_scenario))
        .route(
            "/test-assets/projects/{project}/scenarios/{id}",
            put(update_scenario).delete(delete_scenario),
        )
        .route("/test-assets/projects/{project}/scenarios/{id}/runs", get(runs).post(record_run))
        .with_state(AssetState { store, source, sessions })
}
fn response(result: AssetResult<Value>) -> Response {
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => {
            let code = match &e {
                AssetError::Invalid(_) => StatusCode::BAD_REQUEST,
                AssetError::Forbidden => StatusCode::FORBIDDEN,
                AssetError::NotFound => StatusCode::NOT_FOUND,
                AssetError::Conflict(_) => StatusCode::CONFLICT,
                AssetError::Storage => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (code, Json(json!({"error":e.to_string()}))).into_response()
        }
    }
}
async fn gate(s: &AssetState, u: &AuthUser, p: &str, write: bool) -> AssetResult<()> {
    if !u.can("FUNCTIONAL_CASE", if write { "ADD" } else { "READ" }) || !u.can("PROJECT", "READ") {
        return Err(AssetError::Forbidden);
    }
    s.store.authorize(p, &u.user_id, u.can("SYSTEM_USER", "UPDATE")).await
}
async fn manager(s: &AssetState, u: &AuthUser, p: &str) -> AssetResult<()> {
    gate(s, u, p, true).await?;
    if !u.can("PROJECT", "UPDATE") {
        return Err(AssetError::Forbidden);
    }
    Ok(())
}
async fn overview(u: AuthUser, State(s): State<AssetState>, Path(p): Path<String>) -> Response {
    response(async{gate(&s,&u,&p,false).await?;let mut data=s.store.overview(&p).await?;if !u.can("REQUIREMENT","READ"){data["imports"]=json!([]);}data["permissions"]=json!({"edit":u.can("FUNCTIONAL_CASE","ADD"),"manage":u.can("PROJECT","UPDATE")&&u.can("FUNCTIONAL_CASE","ADD"),"import":u.can("REQUIREMENT","ADD")&&u.can("REQUIREMENT","READ")&&u.can("PROJECT","UPDATE")&&u.can("FUNCTIONAL_CASE","ADD")});Ok(data)}.await)
}
async fn sources(u: AuthUser, State(s): State<AssetState>, Path(p): Path<String>) -> Response {
    response(
        async {
            gate(&s, &u, &p, false).await?;
            if !u.can("REQUIREMENT", "READ") {
                return Err(AssetError::Forbidden);
            }
            match &s.source {
                Some(source) => source.sources().await,
                None => Ok(json!({"items":[],"enabled":false})),
            }
        }
        .await,
    )
}
async fn create_service(
    u: AuthUser,
    State(s): State<AssetState>,
    Path(p): Path<String>,
    Json(b): Json<ServiceInput>,
) -> Response {
    response(
        async {
            manager(&s, &u, &p).await?;
            s.store.save_service(&p, None, &b).await
        }
        .await,
    )
}
async fn update_service(
    u: AuthUser,
    State(s): State<AssetState>,
    Path((p, id)): Path<(String, String)>,
    Json(b): Json<ServiceInput>,
) -> Response {
    response(
        async {
            manager(&s, &u, &p).await?;
            s.store.save_service(&p, Some(&id), &b).await
        }
        .await,
    )
}
async fn delete_service(
    u: AuthUser,
    State(s): State<AssetState>,
    Path((p, id)): Path<(String, String)>,
) -> Response {
    response(
        async {
            manager(&s, &u, &p).await?;
            s.store.delete_service(&p, &id).await?;
            Ok(json!({"deleted":true}))
        }
        .await,
    )
}
async fn assign(
    u: AuthUser,
    State(s): State<AssetState>,
    Path(p): Path<String>,
    Json(b): Json<Value>,
) -> Response {
    response(
        async {
            manager(&s, &u, &p).await?;
            let cases: Vec<String> = serde_json::from_value(b["caseIds"].clone())
                .map_err(|_| AssetError::Invalid("caseIds must be a string array".into()))?;
            s.store
                .assign_cases(&p, b["serviceId"].as_str().filter(|s| !s.is_empty()), &cases)
                .await?;
            Ok(json!({"assigned":cases.len()}))
        }
        .await,
    )
}
async fn mapping(
    u: AuthUser,
    State(s): State<AssetState>,
    Path(p): Path<String>,
    Json(b): Json<Value>,
) -> Response {
    response(
        async {
            manager(&s, &u, &p).await?;
            s.store.save_mapping(&p, &b).await?;
            Ok(json!({"saved":true}))
        }
        .await,
    )
}
async fn preview(
    u: AuthUser,
    State(s): State<AssetState>,
    Path(p): Path<String>,
    Json(b): Json<Value>,
) -> Response {
    response(
        async {
            manager(&s, &u, &p).await?;
            if !u.can("REQUIREMENT", "ADD") || !u.can("REQUIREMENT", "READ") {
                return Err(AssetError::Forbidden);
            }
            let source = s
                .source
                .as_ref()
                .ok_or_else(|| AssetError::Conflict("source plugin is disabled".into()))?;
            let id = b["sourceId"]
                .as_str()
                .ok_or_else(|| AssetError::Invalid("sourceId is required".into()))?;
            let snapshot = source.snapshot(id).await?;
            s.store.preview_import(&p, &u.user_id, &snapshot).await
        }
        .await,
    )
}
async fn import_item(
    u: AuthUser,
    State(s): State<AssetState>,
    Path((p, id)): Path<(String, String)>,
    Query(q): Query<std::collections::BTreeMap<String, String>>,
) -> Response {
    response(
        async {
            gate(&s, &u, &p, false).await?;
            if !u.can("REQUIREMENT", "READ") {
                return Err(AssetError::Forbidden);
            }
            s.store
                .import_item(
                    &p,
                    &id,
                    q.get("kind").map(String::as_str).unwrap_or(""),
                    q.get("externalId").map(String::as_str).unwrap_or(""),
                )
                .await
        }
        .await,
    )
}
async fn apply(
    u: AuthUser,
    State(s): State<AssetState>,
    Path((p, id)): Path<(String, String)>,
    Json(b): Json<Value>,
) -> Response {
    response(
        async {
            manager(&s, &u, &p).await?;
            if !u.can("REQUIREMENT", "ADD") || !u.can("REQUIREMENT", "READ") {
                return Err(AssetError::Forbidden);
            }
            let source = s
                .source
                .as_ref()
                .ok_or_else(|| AssetError::Conflict("source plugin is disabled".into()))?;
            let batch = s.store.import_batch(&p, &id).await?;
            let source_id = batch["source_id"].as_str().ok_or(AssetError::Storage)?;
            let snapshot = source.snapshot(source_id).await?;
            s.store.apply_import(&p, &u.user_id, &id, &snapshot.revision, &b["resolutions"]).await
        }
        .await,
    )
}
async fn create_scenario(
    u: AuthUser,
    State(s): State<AssetState>,
    Path(p): Path<String>,
    Json(b): Json<ScenarioInput>,
) -> Response {
    response(
        async {
            gate(&s, &u, &p, true).await?;
            if !b.requirement_ids.is_empty() && !u.can("REQUIREMENT", "READ") {
                return Err(AssetError::Forbidden);
            }
            if b.api_scenario_id.as_deref().is_some_and(|v| !v.is_empty())
                && !u.can("API_SCENARIO", "READ")
            {
                return Err(AssetError::Forbidden);
            }
            s.store.save_scenario(&p, &u.user_id, None, &b).await
        }
        .await,
    )
}
async fn update_scenario(
    u: AuthUser,
    State(s): State<AssetState>,
    Path((p, id)): Path<(String, String)>,
    Json(b): Json<ScenarioInput>,
) -> Response {
    response(
        async {
            gate(&s, &u, &p, true).await?;
            if !b.requirement_ids.is_empty() && !u.can("REQUIREMENT", "READ") {
                return Err(AssetError::Forbidden);
            }
            if b.api_scenario_id.as_deref().is_some_and(|v| !v.is_empty())
                && !u.can("API_SCENARIO", "READ")
            {
                return Err(AssetError::Forbidden);
            }
            s.store.save_scenario(&p, &u.user_id, Some(&id), &b).await
        }
        .await,
    )
}
async fn delete_scenario(
    u: AuthUser,
    State(s): State<AssetState>,
    Path((p, id)): Path<(String, String)>,
) -> Response {
    response(
        async {
            gate(&s, &u, &p, true).await?;
            s.store.delete_scenario(&p, &id).await?;
            Ok(json!({"deleted":true}))
        }
        .await,
    )
}
async fn runs(
    u: AuthUser,
    State(s): State<AssetState>,
    Path((p, id)): Path<(String, String)>,
) -> Response {
    response(
        async {
            gate(&s, &u, &p, false).await?;
            s.store.scenario_runs(&p, &id).await
        }
        .await,
    )
}
async fn record_run(
    u: AuthUser,
    State(s): State<AssetState>,
    Path((p, id)): Path<(String, String)>,
    Json(b): Json<Value>,
) -> Response {
    response(
        async {
            gate(&s, &u, &p, true).await?;
            s.store.record_run(&p, &u.user_id, &id, &b).await
        }
        .await,
    )
}
async fn save_inventory(
    u: AuthUser,
    State(s): State<AssetState>,
    Path((p, id)): Path<(String, String)>,
    Json(b): Json<Value>,
) -> Response {
    response(
        async {
            manager(&s, &u, &p).await?;
            if b["endpoints"].as_array().into_iter().flatten().any(|e| {
                e["apiDefinitionId"].as_str().is_some_and(|v| !v.is_empty())
                    || e["apiCaseIds"].as_array().is_some_and(|v| !v.is_empty())
            }) && !u.can("API_DEFINITION", "READ")
            {
                return Err(AssetError::Forbidden);
            }
            s.store.save_inventory(&p, &u.user_id, &id, &b).await
        }
        .await,
    )
}

async fn set_case_api(
    u: AuthUser,
    State(s): State<AssetState>,
    Path((p, id)): Path<(String, String)>,
    Json(b): Json<Value>,
) -> Response {
    response(
        async {
            gate(&s, &u, &p, true).await?;
            if !u.can("API_DEFINITION", "READ") {
                return Err(AssetError::Forbidden);
            }
            let links = b["links"]
                .as_array()
                .ok_or_else(|| AssetError::Invalid("links array required".into()))?;
            s.store.set_case_api(&p, &id, links).await
        }
        .await,
    )
}
