use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{FromRef, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use calamine::{Reader, Xlsx};
use rust_xlsxwriter::Workbook;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use webauth::{AuthUser, SessionStore};

use crate::application::{
    export_rows, CreateCaseError, CreateCaseUseCase, DeleteCaseUseCase, ImportCasesUseCase,
    ListCasesUseCase, UpdateCaseUseCase,
};
use crate::domain::{CaseStep, FunctionalCase};
use crate::ports::CaseRepository;

#[derive(Clone)]
struct CaseState {
    create: CreateCaseUseCase,
    update: UpdateCaseUseCase,
    delete: DeleteCaseUseCase,
    list: ListCasesUseCase,
    import: ImportCasesUseCase,
    repo: Arc<dyn CaseRepository>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<CaseState> for Arc<dyn SessionStore> {
    fn from_ref(s: &CaseState) -> Self {
        s.sessions.clone()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn router(
    create: CreateCaseUseCase,
    update: UpdateCaseUseCase,
    delete: DeleteCaseUseCase,
    list: ListCasesUseCase,
    import: ImportCasesUseCase,
    repo: Arc<dyn CaseRepository>,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/functional-case", post(create_case).get(list_cases))
        .route("/functional-case/export", get(export_cases))
        .route("/functional-case/import", post(import_cases))
        .route("/functional-case/{id}", put(update_case).delete(delete_case))
        .route("/functional-case/{id}/requirements", get(case_requirements))
        .route("/requirement-case/link", post(link_req_case))
        .route("/requirement-case/unlink", post(unlink_req_case))
        .route("/requirement/{id}/functional-coverage", get(requirement_coverage))
        .with_state(CaseState { create, update, delete, list, import, repo, sessions })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReqCaseLinkBody {
    requirement_id: String,
    criterion_index: i32,
    functional_case_id: String,
    #[serde(default)]
    project_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CoverageCaseDto {
    criterion_index: i32,
    case_id: String,
    case_name: String,
    module: String,
    priority: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CaseRequirementDto {
    requirement_id: String,
    requirement_title: String,
    criterion_index: i32,
}

async fn link_req_case(
    user: AuthUser,
    State(st): State<CaseState>,
    Json(b): Json<ReqCaseLinkBody>,
) -> Response {
    if !user.can("FUNCTIONAL_CASE", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st
        .repo
        .link_requirement_case(
            &b.requirement_id,
            b.criterion_index,
            &b.functional_case_id,
            &b.project_id,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

async fn unlink_req_case(
    user: AuthUser,
    State(st): State<CaseState>,
    Json(b): Json<ReqCaseLinkBody>,
) -> Response {
    if !user.can("FUNCTIONAL_CASE", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st
        .repo
        .unlink_requirement_case(&b.requirement_id, b.criterion_index, &b.functional_case_id)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

async fn requirement_coverage(
    user: AuthUser,
    State(st): State<CaseState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("FUNCTIONAL_CASE", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.repo.cases_for_requirement(&id).await {
        Ok(rows) => {
            let items: Vec<CoverageCaseDto> = rows
                .into_iter()
                .map(|r| CoverageCaseDto {
                    criterion_index: r.criterion_index,
                    case_id: r.case_id,
                    case_name: r.case_name,
                    module: r.module,
                    priority: r.priority,
                })
                .collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

async fn case_requirements(
    user: AuthUser,
    State(st): State<CaseState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("FUNCTIONAL_CASE", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.repo.requirements_for_case(&id).await {
        Ok(rows) => {
            let items: Vec<CaseRequirementDto> = rows
                .into_iter()
                .map(|r| CaseRequirementDto {
                    requirement_id: r.requirement_id,
                    requirement_title: r.requirement_title,
                    criterion_index: r.criterion_index,
                })
                .collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
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
    steps: Vec<CaseStepDto>,
    created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CaseStepDto {
    step: String,
    expected: String,
}

impl From<CaseStep> for CaseStepDto {
    fn from(s: CaseStep) -> Self {
        Self { step: s.step, expected: s.expected }
    }
}

impl From<CaseStepDto> for CaseStep {
    fn from(s: CaseStepDto) -> Self {
        Self { step: s.step, expected: s.expected }
    }
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
            steps: c.steps.into_iter().map(CaseStepDto::from).collect(),
            created_by: c.created_by,
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
    #[serde(default)]
    steps: Vec<CaseStepDto>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct ProjectQuery {
    project_id: String,
}

#[utoipa::path(post, path = "/functional-case", tag = "functional-case", request_body = CaseBody, responses((status = 201, body = CaseResponse), (status = 400), (status = 403)), security(("bearer" = [])))]
async fn create_case(
    user: AuthUser,
    State(st): State<CaseState>,
    Json(b): Json<CaseBody>,
) -> Response {
    if !user.can("FUNCTIONAL_CASE", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let steps: Vec<CaseStep> = b.steps.into_iter().map(CaseStep::from).collect();
    match st
        .create
        .execute(
            &b.project_id,
            &b.name,
            &b.module,
            &b.priority,
            &b.status,
            b.custom_fields,
            steps,
            Some(user.user_id.as_str()),
        )
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

#[utoipa::path(put, path = "/functional-case/{id}", tag = "functional-case", request_body = CaseBody, responses((status = 200, body = CaseResponse), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_case(
    user: AuthUser,
    State(st): State<CaseState>,
    Path(id): Path<String>,
    Json(b): Json<CaseBody>,
) -> Response {
    if !user.can("FUNCTIONAL_CASE", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let steps: Vec<CaseStep> = b.steps.into_iter().map(CaseStep::from).collect();
    match st
        .update
        .execute(
            &id,
            &b.project_id,
            &b.name,
            &b.module,
            &b.priority,
            &b.status,
            b.custom_fields,
            steps,
        )
        .await
    {
        Ok(Some(c)) => (StatusCode::OK, Json(CaseResponse::from(c))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "case not found").into_response(),
        Err(CreateCaseError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid case payload").into_response()
        }
        Err(CreateCaseError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

#[utoipa::path(delete, path = "/functional-case/{id}", tag = "functional-case", responses((status = 204), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn delete_case(
    user: AuthUser,
    State(st): State<CaseState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("FUNCTIONAL_CASE", "DELETE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.delete.execute(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "case not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(get, path = "/functional-case", tag = "functional-case", params(ProjectQuery), responses((status = 200, body = [CaseResponse]), (status = 403)), security(("bearer" = [])))]
async fn list_cases(
    user: AuthUser,
    State(st): State<CaseState>,
    Query(q): Query<ProjectQuery>,
) -> Response {
    if !user.can("FUNCTIONAL_CASE", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.list.execute(&q.project_id).await {
        Ok(list) => {
            let items: Vec<CaseResponse> = list.into_iter().map(CaseResponse::from).collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

#[utoipa::path(get, path = "/functional-case/export", tag = "functional-case", params(ProjectQuery), responses((status = 200, description = "xlsx 文件"), (status = 403)), security(("bearer" = [])))]
async fn export_cases(
    user: AuthUser,
    State(st): State<CaseState>,
    Query(q): Query<ProjectQuery>,
) -> Response {
    if !user.can("FUNCTIONAL_CASE", "READ") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let cases = match st.list.execute(&q.project_id).await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    };
    match rows_to_xlsx(&export_rows(&cases)) {
        Ok(bytes) => (
            StatusCode::OK,
            [
                (
                    header::CONTENT_TYPE,
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                ),
                (header::CONTENT_DISPOSITION, "attachment; filename=\"cases.xlsx\""),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "xlsx error").into_response(),
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    imported: usize,
}

#[utoipa::path(post, path = "/functional-case/import", tag = "functional-case", params(ProjectQuery), request_body(content = Vec<u8>, description = "xlsx 文件字节"), responses((status = 200, body = ImportResult), (status = 400), (status = 403)), security(("bearer" = [])))]
async fn import_cases(
    user: AuthUser,
    State(st): State<CaseState>,
    Query(q): Query<ProjectQuery>,
    body: Bytes,
) -> Response {
    if !user.can("FUNCTIONAL_CASE", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let rows = match xlsx_to_rows(&body) {
        Ok(r) => r,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid xlsx").into_response(),
    };
    match st.import.execute(&q.project_id, &rows, Some(user.user_id.as_str())).await {
        Ok(n) => (StatusCode::OK, Json(ImportResult { imported: n })).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

fn xlsx_to_rows(bytes: &[u8]) -> Result<Vec<Vec<String>>, String> {
    let mut wb: Xlsx<Cursor<Vec<u8>>> =
        calamine::open_workbook_from_rs(Cursor::new(bytes.to_vec()))
            .map_err(|e: calamine::XlsxError| e.to_string())?;
    let range = wb
        .worksheet_range_at(0)
        .ok_or_else(|| "no worksheet".to_string())?
        .map_err(|e: calamine::XlsxError| e.to_string())?;
    Ok(range.rows().map(|r| r.iter().map(|c| c.to_string()).collect()).collect())
}

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
    paths(create_case, update_case, delete_case, list_cases, export_cases, import_cases),
    components(schemas(CaseBody, CaseResponse, ImportResult)),
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
            UpdateCaseUseCase::new(repo.clone()),
            DeleteCaseUseCase::new(repo.clone()),
            ListCasesUseCase::new(repo.clone()),
            ImportCasesUseCase::new(repo.clone()),
            repo,
            sessions,
        );
        (r, token)
    }

    fn post(uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b =
            Request::builder().method("POST").uri(uri).header("content-type", "application/json");
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

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/functional-case/export?projectId=p1")
                    .header("authorization", format!("Bearer {t}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        assert!(bytes.len() > 100);
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[tokio::test]
    async fn import_xlsx_round_trip() {
        let (app, t) = app("FUNCTIONAL_CASE:READ+ADD").await;
        let rows = vec![
            vec!["名称".to_string(), "优先级".to_string(), "owner".to_string()],
            vec!["登录成功".into(), "P0".into(), "alice".into()],
            vec!["密码错误".into(), "P1".into(), "bob".into()],
        ];
        let xlsx = rows_to_xlsx(&rows).expect("xlsx");
        let req = Request::builder()
            .method("POST")
            .uri("/functional-case/import?projectId=p1")
            .header("authorization", format!("Bearer {t}"))
            .body(Body::from(xlsx))
            .expect("req");
        let resp = app.clone().oneshot(req).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let v: serde_json::Value = {
            let b = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
            serde_json::from_slice(&b).expect("json")
        };
        assert_eq!(v["imported"], 2);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/functional-case?projectId=p1")
                    .header("authorization", format!("Bearer {t}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        let b = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let list: serde_json::Value = serde_json::from_slice(&b).expect("json");
        assert_eq!(list.as_array().expect("arr").len(), 2);
        assert_eq!(list[0]["customFields"]["owner"], "alice");
    }

    #[tokio::test]
    async fn update_and_delete_round_trip() {
        let (app, t) = app("FUNCTIONAL_CASE:READ+ADD+UPDATE+DELETE").await;
        let resp = app
            .clone()
            .oneshot(post("/functional-case", r#"{"projectId":"p1","name":"登录成功"}"#, Some(&t)))
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let created: serde_json::Value = {
            let b = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
            serde_json::from_slice(&b).expect("json")
        };
        let id = created["id"].as_str().expect("id").to_string();

        let put = Request::builder()
            .method("PUT")
            .uri(format!("/functional-case/{id}"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {t}"))
            .body(Body::from(r#"{"projectId":"p1","name":"登录失败","priority":"P0"}"#.to_string()))
            .expect("req");
        let resp = app.clone().oneshot(put).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let updated: serde_json::Value = {
            let b = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
            serde_json::from_slice(&b).expect("json")
        };
        assert_eq!(updated["name"], "登录失败");
        assert_eq!(updated["priority"], "P0");

        let del = Request::builder()
            .method("DELETE")
            .uri(format!("/functional-case/{id}"))
            .header("authorization", format!("Bearer {t}"))
            .body(Body::empty())
            .expect("req");
        assert_eq!(app.clone().oneshot(del).await.expect("resp").status(), StatusCode::NO_CONTENT);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/functional-case?projectId=p1")
                    .header("authorization", format!("Bearer {t}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        let b = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let list: serde_json::Value = serde_json::from_slice(&b).expect("json");
        assert_eq!(list.as_array().expect("arr").len(), 0);
    }

    #[tokio::test]
    async fn update_requires_update_permission() {
        let (app, t) = app("FUNCTIONAL_CASE:READ+ADD").await;
        let put = Request::builder()
            .method("PUT")
            .uri("/functional-case/whatever")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {t}"))
            .body(Body::from(r#"{"projectId":"p1","name":"x"}"#.to_string()))
            .expect("req");
        assert_eq!(app.oneshot(put).await.expect("resp").status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_requires_delete_permission() {
        let (app, t) = app("FUNCTIONAL_CASE:READ+ADD").await;
        let del = Request::builder()
            .method("DELETE")
            .uri("/functional-case/whatever")
            .header("authorization", format!("Bearer {t}"))
            .body(Body::empty())
            .expect("req");
        assert_eq!(app.oneshot(del).await.expect("resp").status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn import_requires_add_permission() {
        let (app, t) = app("FUNCTIONAL_CASE:READ").await;
        let xlsx = rows_to_xlsx(&[vec!["名称".to_string()], vec!["x".to_string()]]).expect("xlsx");
        let req = Request::builder()
            .method("POST")
            .uri("/functional-case/import?projectId=p1")
            .header("authorization", format!("Bearer {t}"))
            .body(Body::from(xlsx))
            .expect("req");
        assert_eq!(app.oneshot(req).await.expect("resp").status(), StatusCode::FORBIDDEN);
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
