//! 功能用例 HTTP 适配器:CRUD + Excel 导出。
//!
//! RBAC 资源串 `FUNCTIONAL_CASE`:创建需 ADD;读(列出/导出)开放。
//! 导出用 rust_xlsxwriter 把 `export_rows` 的纯行编码成 .xlsx 字节。

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{FromRef, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rust_xlsxwriter::Workbook;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use crate::application::{
    export_rows, CreateCaseError, CreateCaseUseCase, ListCasesUseCase,
};
use crate::domain::FunctionalCase;

#[derive(Clone)]
struct CaseState {
    create: CreateCaseUseCase,
    list: ListCasesUseCase,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<CaseState> for Arc<dyn SessionStore> {
    fn from_ref(s: &CaseState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    create: CreateCaseUseCase,
    list: ListCasesUseCase,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/functional-case", post(create_case).get(list_cases))
        .route("/functional-case/export", get(export_cases))
        .with_state(CaseState { create, list, sessions })
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CaseResponse {
    id: String,
    project_id: String,
    name: String,
    module: String,
    priority: String,
    status: String,
    custom_fields: BTreeMap<String, String>,
}

impl From<FunctionalCase> for CaseResponse {
    fn from(c: FunctionalCase) -> Self {
        Self {
            id: c.id,
            project_id: c.project_id,
            name: c.name,
            module: c.module,
            priority: c.priority,
            status: c.status,
            custom_fields: c.custom_fields,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CaseBody {
    project_id: String,
    name: String,
    #[serde(default)]
    module: String,
    #[serde(default)]
    priority: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    custom_fields: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct ProjectQuery {
    project_id: String,
}

#[utoipa::path(post, path = "/functional-case", tag = "functional-case", request_body = CaseBody, responses((status = 201, body = CaseResponse), (status = 400), (status = 403)), security(("bearer" = [])))]
async fn create_case(user: AuthUser, State(st): State<CaseState>, Json(b): Json<CaseBody>) -> Response {
    if !user.can("FUNCTIONAL_CASE", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st
        .create
        .execute(&b.project_id, &b.name, &b.module, &b.priority, &b.status, b.custom_fields)
        .await
    {
        Ok(c) => (StatusCode::CREATED, Json(CaseResponse::from(c))).into_response(),
        Err(CreateCaseError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid case payload").into_response()
        }
        Err(CreateCaseError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(get, path = "/functional-case", tag = "functional-case", params(ProjectQuery), responses((status = 200, body = [CaseResponse])))]
async fn list_cases(State(st): State<CaseState>, Query(q): Query<ProjectQuery>) -> Response {
    match st.list.execute(&q.project_id).await {
        Ok(list) => {
            let items: Vec<CaseResponse> = list.into_iter().map(CaseResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(get, path = "/functional-case/export", tag = "functional-case", params(ProjectQuery), responses((status = 200, description = "xlsx 文件")))]
async fn export_cases(State(st): State<CaseState>, Query(q): Query<ProjectQuery>) -> Response {
    let cases = match st.list.execute(&q.project_id).await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    };
    match rows_to_xlsx(&export_rows(&cases)) {
        Ok(bytes) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                (header::CONTENT_DISPOSITION, "attachment; filename=\"cases.xlsx\""),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "xlsx error").into_response(),
    }
}

/// 纯行(表头 + 数据行)→ xlsx 字节(首行加粗作表头)。
fn rows_to_xlsx(rows: &[Vec<String>]) -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> {
    let mut wb = Workbook::new();
    let sheet = wb.add_worksheet();
    let bold = rust_xlsxwriter::Format::new().set_bold();
    for (r, row) in rows.iter().enumerate() {
        for (c, val) in row.iter().enumerate() {
            if r == 0 {
                sheet.write_string_with_format(r as u32, c as u16, val, &bold)?;
            } else {
                sheet.write_string(r as u32, c as u16, val)?;
            }
        }
    }
    wb.save_to_buffer()
}

#[derive(OpenApi)]
#[openapi(
    paths(create_case, list_cases, export_cases),
    components(schemas(CaseBody, CaseResponse)),
    tags((name = "functional-case", description = "功能用例"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryCaseRepository;
    use axum::body::Body;
    use axum::http::Request;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app(perms: &str) -> (Router, String) {
        let repo = Arc::new(InMemoryCaseRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let set = PermissionSet::from_raw([perms.to_string()]).expect("perms");
        let token = sessions.create("u", set, 3600).await.expect("token");
        let r = router(
            CreateCaseUseCase::new(repo.clone()),
            ListCasesUseCase::new(repo),
            sessions,
        );
        (r, token)
    }

    fn post(uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method("POST").uri(uri).header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).expect("req")
    }

    #[tokio::test]
    async fn create_list_and_export_xlsx() {
        let (app, t) = app("FUNCTIONAL_CASE:READ+ADD").await;
        let resp = app
            .clone()
            .oneshot(post(
                "/functional-case",
                r#"{"projectId":"p1","name":"登录成功","module":"登录","customFields":{"owner":"alice"}}"#,
                Some(&t),
            ))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // 导出 xlsx:验证返回的是合法 xlsx(zip 魔数 PK)。
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/functional-case/export?projectId=p1")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        assert!(bytes.len() > 100);
        assert_eq!(&bytes[0..2], b"PK"); // xlsx = zip
    }

    #[tokio::test]
    async fn create_requires_add_permission() {
        let (app, t) = app("FUNCTIONAL_CASE:READ").await;
        let resp = app
            .oneshot(post("/functional-case", r#"{"projectId":"p1","name":"x"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn create_without_token_401() {
        let (app, _t) = app("FUNCTIONAL_CASE:ADD").await;
        let resp = app
            .oneshot(post("/functional-case", r#"{"projectId":"p1","name":"x"}"#, None))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
