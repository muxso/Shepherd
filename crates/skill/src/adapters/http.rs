use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use crate::application::{CreateSkillError, CreateSkillUseCase, SkillCmdError, SkillService};
use crate::domain::Skill;

#[derive(Clone)]
struct SkillState {
    create: CreateSkillUseCase,
    admin: SkillService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<SkillState> for Arc<dyn SessionStore> {
    fn from_ref(s: &SkillState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    create: CreateSkillUseCase,
    admin: SkillService,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/skill", post(create_skill).get(list_skills))
        .route("/skill/compose", post(compose))
        .route("/skill/{id}", get(get_skill).put(update_skill).delete(delete_skill))
        .with_state(SkillState { create, admin, sessions })
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SkillCreateBody)]
struct CreateBody {
    project_id: String,
    name: String,
    #[serde(default)]
    description: String,
    instructions: String,
    #[serde(default)]
    includes: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct UpdateBody {
    name: String,
    #[serde(default)]
    description: String,
    instructions: String,
    #[serde(default)]
    includes: Vec<String>,
    #[serde(default = "tru")]
    enabled: bool,
}
fn tru() -> bool {
    true
}

#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    project_id: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ComposeBody {
    project_id: String,
    skill_ids: Vec<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct SkillResponse {
    id: String,
    project_id: String,
    name: String,
    description: String,
    instructions: String,
    includes: Vec<String>,
    enabled: bool,
}

impl From<&Skill> for SkillResponse {
    fn from(s: &Skill) -> Self {
        Self {
            id: s.id.clone(),
            project_id: s.project_id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            instructions: s.instructions.clone(),
            includes: s.includes.clone(),
            enabled: s.enabled,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ComposeResponse {
    skill_ids: Vec<String>,
    instructions: String,
}

fn cmd_err(e: SkillCmdError) -> Response {
    match e {
        SkillCmdError::NotFound => (StatusCode::NOT_FOUND, "skill not found").into_response(),
        SkillCmdError::NameExists => (StatusCode::CONFLICT, "name already exists").into_response(),
        SkillCmdError::Validation(_) => (StatusCode::BAD_REQUEST, "invalid skill").into_response(),
        SkillCmdError::Compose(_) => {
            (StatusCode::CONFLICT, "skill composition error (unknown include or cycle)")
                .into_response()
        }
        SkillCmdError::Repo(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(post, path = "/skill", tag = "skill", request_body = CreateBody, responses((status = 201, body = SkillResponse), (status = 409)), security(("bearer" = [])))]
async fn create_skill(
    user: AuthUser,
    State(st): State<SkillState>,
    Json(b): Json<CreateBody>,
) -> Response {
    if !user.can("SKILL", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st
        .create
        .execute(&b.project_id, &b.name, &b.description, &b.instructions, &b.includes)
        .await
    {
        Ok(s) => (StatusCode::CREATED, Json(SkillResponse::from(&s))).into_response(),
        Err(CreateSkillError::NameAlreadyExists) => {
            (StatusCode::CONFLICT, "name already exists").into_response()
        }
        Err(CreateSkillError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid skill").into_response()
        }
        Err(CreateSkillError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/skill", tag = "skill", params(ListQuery), responses((status = 200, body = [SkillResponse])))]
async fn list_skills(State(st): State<SkillState>, Query(q): Query<ListQuery>) -> Response {
    match st.admin.list(&q.project_id).await {
        Ok(items) => {
            let body: Vec<SkillResponse> = items.iter().map(SkillResponse::from).collect();
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(get, path = "/skill/{id}", tag = "skill", params(("id" = String, Path)), responses((status = 200, body = SkillResponse), (status = 404)))]
async fn get_skill(State(st): State<SkillState>, Path(id): Path<String>) -> Response {
    match st.admin.get(&id).await {
        Ok(s) => (StatusCode::OK, Json(SkillResponse::from(&s))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(put, path = "/skill/{id}", tag = "skill", params(("id" = String, Path)), request_body = UpdateBody, responses((status = 200, body = SkillResponse), (status = 409)), security(("bearer" = [])))]
async fn update_skill(
    user: AuthUser,
    State(st): State<SkillState>,
    Path(id): Path<String>,
    Json(b): Json<UpdateBody>,
) -> Response {
    if !user.can("SKILL", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st
        .admin
        .update(&id, &b.name, &b.description, &b.instructions, b.includes, b.enabled)
        .await
    {
        Ok(s) => (StatusCode::OK, Json(SkillResponse::from(&s))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(delete, path = "/skill/{id}", tag = "skill", params(("id" = String, Path)), responses((status = 204), (status = 404)), security(("bearer" = [])))]
async fn delete_skill(
    user: AuthUser,
    State(st): State<SkillState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("SKILL", "DELETE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.delete(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => cmd_err(e),
    }
}

#[utoipa::path(post, path = "/skill/compose", tag = "skill", request_body = ComposeBody, responses((status = 200, body = ComposeResponse), (status = 409)), security(("bearer" = [])))]
async fn compose(
    user: AuthUser,
    State(st): State<SkillState>,
    Json(b): Json<ComposeBody>,
) -> Response {
    if !user.can("SKILL", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.compose(&b.project_id, &b.skill_ids).await {
        Ok(c) => (
            StatusCode::OK,
            Json(ComposeResponse { skill_ids: c.skill_ids, instructions: c.instructions }),
        )
            .into_response(),
        Err(e) => cmd_err(e),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(create_skill, list_skills, compose, get_skill, update_skill, delete_skill),
    components(schemas(CreateBody, UpdateBody, ComposeBody, SkillResponse, ComposeResponse)),
    tags((name = "skill", description = "AI Skill 编排"))
)]
struct ApiDoc;
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemorySkillRepository;
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app_with(perms: &str) -> (Router, String) {
        let repo = Arc::new(InMemorySkillRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let set = PermissionSet::from_raw([perms.to_string()]).expect("perms");
        let token = sessions.create("u", set, 3600).await.expect("token");
        let r = router(CreateSkillUseCase::new(repo.clone()), SkillService::new(repo), sessions);
        (r, token)
    }

    fn req(method: &str, uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b =
            Request::builder().method(method).uri(uri).header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    async fn json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    #[tokio::test]
    async fn create_compose_flow() {
        let (app, t) = app_with("SKILL:READ+ADD+UPDATE+DELETE").await;
        let a = app
            .clone()
            .oneshot(req(
                "POST",
                "/skill",
                r#"{"projectId":"p1","name":"基础","instructions":"遵循六边形"}"#,
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(a.status(), StatusCode::CREATED);
        let aid = json(a).await["id"].as_str().expect("id").to_string();
        let b = app.clone().oneshot(req("POST", "/skill", &format!(r#"{{"projectId":"p1","name":"Rust","instructions":"用 thiserror","includes":["{aid}"]}}"#), Some(&t))).await.expect("r");
        let bid = json(b).await["id"].as_str().expect("id").to_string();

        let c = app
            .clone()
            .oneshot(req(
                "POST",
                "/skill/compose",
                &format!(r#"{{"projectId":"p1","skillIds":["{bid}"]}}"#),
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(c.status(), StatusCode::OK);
        let cv = json(c).await;
        assert_eq!(cv["skillIds"][0], aid);
        assert!(cv["instructions"].as_str().expect("s").contains("遵循六边形"));
        assert!(cv["instructions"].as_str().expect("s").contains("用 thiserror"));

        assert_eq!(
            app.clone()
                .oneshot(req("GET", "/skill?projectId=p1", "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.oneshot(req("DELETE", &format!("/skill/{aid}"), "", Some(&t)))
                .await
                .expect("r")
                .status(),
            StatusCode::NO_CONTENT
        );
    }

    #[tokio::test]
    async fn compose_cycle_is_conflict() {
        let (app, t) = app_with("SKILL:READ+ADD+UPDATE").await;
        let a = json(
            app.clone()
                .oneshot(req(
                    "POST",
                    "/skill",
                    r#"{"projectId":"p1","name":"A","instructions":"a"}"#,
                    Some(&t),
                ))
                .await
                .expect("r"),
        )
        .await["id"]
            .as_str()
            .unwrap()
            .to_string();
        let b = json(
            app.clone()
                .oneshot(req(
                    "POST",
                    "/skill",
                    r#"{"projectId":"p1","name":"B","instructions":"b"}"#,
                    Some(&t),
                ))
                .await
                .expect("r"),
        )
        .await["id"]
            .as_str()
            .unwrap()
            .to_string();
        app.clone()
            .oneshot(req(
                "PUT",
                &format!("/skill/{a}"),
                &format!(r#"{{"name":"A","instructions":"a","includes":["{b}"]}}"#),
                Some(&t),
            ))
            .await
            .expect("r");
        app.clone()
            .oneshot(req(
                "PUT",
                &format!("/skill/{b}"),
                &format!(r#"{{"name":"B","instructions":"b","includes":["{a}"]}}"#),
                Some(&t),
            ))
            .await
            .expect("r");
        let c = app
            .oneshot(req(
                "POST",
                "/skill/compose",
                &format!(r#"{{"projectId":"p1","skillIds":["{a}"]}}"#),
                Some(&t),
            ))
            .await
            .expect("r");
        assert_eq!(c.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn rbac_create_requires_add() {
        let (app, _t) = app_with("SKILL:READ").await;
        assert_eq!(
            app.oneshot(req(
                "POST",
                "/skill",
                r#"{"projectId":"p1","name":"x","instructions":"y"}"#,
                None
            ))
            .await
            .expect("r")
            .status(),
            StatusCode::UNAUTHORIZED
        );
        let (app, t) = app_with("SKILL:READ").await;
        assert_eq!(
            app.oneshot(req(
                "POST",
                "/skill",
                r#"{"projectId":"p1","name":"x","instructions":"y"}"#,
                Some(&t)
            ))
            .await
            .expect("r")
            .status(),
            StatusCode::FORBIDDEN
        );
    }
}
