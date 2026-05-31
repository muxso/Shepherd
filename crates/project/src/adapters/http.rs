//! 项目管理的 HTTP 适配器:`POST /project`(创建)与 `GET /project`(分页列表)。
//!
//! 同样只做翻译:DTO ↔ 用例,分页参数交给 `kernel::PageRequest` 校验。
//! 业务规则(唯一性、软删除)都在 application/domain,本层一概不碰。
//! 写端点(创建)经 `webauth::AuthUser` 做 RBAC:无令牌→401,缺 `PROJECT:ADD`→403。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use kernel::page::PageRequest;
use crate::application::{CreateProjectError, CreateProjectUseCase, ListProjectsUseCase};
use serde::{Deserialize, Serialize};
use webauth::{AuthUser, SessionStore};

#[derive(Clone)]
struct ProjectState {
    create: CreateProjectUseCase,
    list: ListProjectsUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<ProjectState> for Arc<dyn SessionStore> {
    fn from_ref(s: &ProjectState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    create: CreateProjectUseCase,
    list: ListProjectsUseCase,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/project", post(create_project).get(list_projects))
        .with_state(ProjectState { create, list, sessions })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProjectRequest {
    organization_id: String,
    name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectResponse {
    id: String,
    organization_id: String,
    name: String,
    enable: bool,
}

impl From<crate::domain::Project> for ProjectResponse {
    fn from(p: crate::domain::Project) -> Self {
        Self { id: p.id, organization_id: p.organization_id, name: p.name, enable: p.enable }
    }
}

async fn create_project(
    user: AuthUser,
    State(st): State<ProjectState>,
    Json(req): Json<CreateProjectRequest>,
) -> Response {
    if !user.can("PROJECT", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.create.execute(&req.organization_id, &req.name).await {
        Ok(p) => (StatusCode::CREATED, Json(ProjectResponse::from(p))).into_response(),
        Err(CreateProjectError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid project payload").into_response()
        }
        Err(CreateProjectError::NameAlreadyExists) => {
            (StatusCode::CONFLICT, "project name already exists").into_response()
        }
        Err(CreateProjectError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    organization_id: String,
    #[serde(default = "default_current")]
    current: u32,
    #[serde(default = "default_page_size")]
    page_size: u32,
}

fn default_current() -> u32 {
    1
}
fn default_page_size() -> u32 {
    10
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PageResponse {
    total: u64,
    current: u32,
    page_size: u32,
    total_pages: u64,
    items: Vec<ProjectResponse>,
}

async fn list_projects(State(st): State<ProjectState>, Query(q): Query<ListQuery>) -> Response {
    // 分页参数校验复用 kernel:非法参数 → 400(而不是打到 DB 才炸)
    let page = match PageRequest::new(q.current, q.page_size) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid page params").into_response(),
    };
    match st.list.execute(&q.organization_id, page).await {
        Ok(page) => {
            let body = PageResponse {
                total: page.total,
                current: page.current,
                page_size: page.page_size,
                total_pages: page.total_pages(),
                items: page.items.into_iter().map(ProjectResponse::from).collect(),
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use crate::adapters::InMemoryProjectRepository;
    use kernel::permission::PermissionSet;
    use std::sync::Arc;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    /// 构造 app + 一个拥有 `PROJECT:READ+ADD` 的令牌。
    async fn app() -> (Router, String) {
        let repo = Arc::new(InMemoryProjectRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let perms = PermissionSet::from_raw(["PROJECT:READ+ADD".to_string()]).expect("perms");
        let token = sessions.create("admin", perms, 3600).await.expect("token");
        let r = router(
            CreateProjectUseCase::new(repo.clone()),
            ListProjectsUseCase::new(repo),
            sessions,
        );
        (r, token)
    }

    fn post_json(body: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder()
            .method("POST")
            .uri("/project")
            .header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    async fn status_of(app: Router, req: Request<Body>) -> StatusCode {
        app.oneshot(req).await.expect("resp").status()
    }

    #[tokio::test]
    async fn create_returns_201_then_duplicate_409() {
        let (app, t) = app().await;
        let s1 = status_of(
            app.clone(),
            post_json(r#"{"organizationId":"org1","name":"Demo"}"#, Some(&t)),
        )
        .await;
        assert_eq!(s1, StatusCode::CREATED);

        let s2 =
            status_of(app, post_json(r#"{"organizationId":"org1","name":"Demo"}"#, Some(&t))).await;
        assert_eq!(s2, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn create_without_token_401() {
        let (app, _t) = app().await;
        let s = status_of(app, post_json(r#"{"organizationId":"org1","name":"Demo"}"#, None)).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_without_permission_403() {
        let repo = Arc::new(InMemoryProjectRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        // 只有 READ,无 ADD
        let perms = PermissionSet::from_raw(["PROJECT:READ".to_string()]).expect("perms");
        let token = sessions.create("viewer", perms, 3600).await.expect("token");
        let app = router(
            CreateProjectUseCase::new(repo.clone()),
            ListProjectsUseCase::new(repo),
            sessions,
        );
        let s =
            status_of(app, post_json(r#"{"organizationId":"org1","name":"Demo"}"#, Some(&token)))
                .await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn create_invalid_returns_400() {
        let (app, t) = app().await;
        let s =
            status_of(app, post_json(r#"{"organizationId":"org1","name":"  "}"#, Some(&t))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_returns_paginated_body() {
        let (app, t) = app().await;
        for i in 1..=3 {
            let _ = app
                .clone()
                .oneshot(post_json(
                    &format!(r#"{{"organizationId":"org1","name":"P{i}"}}"#),
                    Some(&t),
                ))
                .await
                .expect("seed");
        }

        // 列表为读端点,无需令牌
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/project?organizationId=org1&current=1&pageSize=2")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["total"], 3);
        assert_eq!(v["totalPages"], 2); // ceil(3/2)
        assert_eq!(v["items"].as_array().expect("arr").len(), 2);
    }

    #[tokio::test]
    async fn list_with_bad_page_params_returns_400() {
        let (app, _t) = app().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/project?organizationId=org1&current=0&pageSize=10")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
