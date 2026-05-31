//! 完整性验证的 HTTP 适配器。DTO ↔ 用例,命令错误映射 HTTP 码。
//!
//! RBAC(资源键 `VERIFICATION`):建验证需 `VERIFICATION:ADD`,建覆盖/同步需 `VERIFICATION:UPDATE`;
//! 读(取验证 / 完整性报告)开放。错误码:校验→400,标准/验证不存在→404。

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use webauth::{AuthUser, SessionStore};

use crate::application::{
    CreateVerificationError, CreateVerificationUseCase, VerificationCmdError, VerificationService,
};
use crate::domain::{CompletenessReport, Verification};

#[derive(Clone)]
struct VerState {
    create: CreateVerificationUseCase,
    admin: VerificationService,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<VerState> for Arc<dyn SessionStore> {
    fn from_ref(s: &VerState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    create: CreateVerificationUseCase,
    admin: VerificationService,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/verification", post(create_verification))
        .route("/verification/{id}", get(get_verification))
        .route("/verification/{id}/report", get(get_report))
        .route("/verification/{id}/link", post(link))
        .route("/verification/{id}/sync", post(sync_task))
        .with_state(VerState { create, admin, sessions })
}

// ---- DTO ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBody {
    requirement_id: String,
    requirement_version: u32,
    #[serde(default)]
    criteria: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinkBody {
    criterion_index: u32,
    decomposition_id: String,
    task_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncBody {
    decomposition_id: String,
    task_id: String,
    satisfied: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LinkResponse {
    decomposition_id: String,
    task_id: String,
    satisfied: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CriterionResponse {
    index: u32,
    text: String,
    status: String,
    links: Vec<LinkResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VerificationResponse {
    id: String,
    requirement_id: String,
    requirement_version: u32,
    complete: bool,
    criteria: Vec<CriterionResponse>,
}

impl From<&Verification> for VerificationResponse {
    fn from(v: &Verification) -> Self {
        Self {
            id: v.id.clone(),
            requirement_id: v.requirement_id.clone(),
            requirement_version: v.requirement_version,
            complete: v.is_complete(),
            criteria: v
                .criteria
                .iter()
                .map(|c| CriterionResponse {
                    index: c.index,
                    text: c.text.clone(),
                    status: c.status().as_str().to_string(),
                    links: c
                        .links
                        .iter()
                        .map(|l| LinkResponse {
                            decomposition_id: l.decomposition_id.clone(),
                            task_id: l.task_id.clone(),
                            satisfied: l.satisfied,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GapResponse {
    criterion_index: u32,
    text: String,
    kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportResponse {
    complete: bool,
    total: usize,
    satisfied: usize,
    gaps: Vec<GapResponse>,
}

impl From<CompletenessReport> for ReportResponse {
    fn from(r: CompletenessReport) -> Self {
        Self {
            complete: r.complete,
            total: r.total,
            satisfied: r.satisfied,
            gaps: r
                .gaps
                .into_iter()
                .map(|g| GapResponse {
                    criterion_index: g.criterion_index,
                    text: g.text,
                    kind: g.kind.as_str().to_string(),
                })
                .collect(),
        }
    }
}

fn cmd_err(e: VerificationCmdError) -> Response {
    match e {
        VerificationCmdError::NotFound => (StatusCode::NOT_FOUND, "verification not found").into_response(),
        VerificationCmdError::NoSuchCriterion(_) => (StatusCode::NOT_FOUND, "criterion not found").into_response(),
        VerificationCmdError::Validation(_) => (StatusCode::BAD_REQUEST, "invalid payload").into_response(),
        VerificationCmdError::Repo(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}

// ---- 处理器 ----

async fn create_verification(user: AuthUser, State(st): State<VerState>, Json(b): Json<CreateBody>) -> Response {
    if !user.can("VERIFICATION", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.create.execute(&b.requirement_id, b.requirement_version, &b.criteria).await {
        Ok(v) => (StatusCode::CREATED, Json(VerificationResponse::from(&v))).into_response(),
        Err(CreateVerificationError::AlreadyExists) => {
            (StatusCode::CONFLICT, "verification already exists").into_response()
        }
        Err(CreateVerificationError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid criteria").into_response()
        }
        Err(CreateVerificationError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}

async fn get_verification(State(st): State<VerState>, Path(id): Path<String>) -> Response {
    match st.admin.get(&id).await {
        Ok(v) => (StatusCode::OK, Json(VerificationResponse::from(&v))).into_response(),
        Err(e) => cmd_err(e),
    }
}

async fn get_report(State(st): State<VerState>, Path(id): Path<String>) -> Response {
    match st.admin.report(&id).await {
        Ok(r) => (StatusCode::OK, Json(ReportResponse::from(r))).into_response(),
        Err(e) => cmd_err(e),
    }
}

async fn link(
    user: AuthUser,
    State(st): State<VerState>,
    Path(id): Path<String>,
    Json(b): Json<LinkBody>,
) -> Response {
    if !user.can("VERIFICATION", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.link(&id, b.criterion_index, &b.decomposition_id, &b.task_id).await {
        Ok(v) => (StatusCode::OK, Json(VerificationResponse::from(&v))).into_response(),
        Err(e) => cmd_err(e),
    }
}

async fn sync_task(
    user: AuthUser,
    State(st): State<VerState>,
    Path(id): Path<String>,
    Json(b): Json<SyncBody>,
) -> Response {
    if !user.can("VERIFICATION", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    match st.admin.sync_task(&id, &b.decomposition_id, &b.task_id, b.satisfied).await {
        Ok(v) => (StatusCode::OK, Json(VerificationResponse::from(&v))).into_response(),
        Err(e) => cmd_err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use crate::adapters::InMemoryVerificationRepository;
    use kernel::permission::PermissionSet;
    use tower::ServiceExt;
    use webauth::testing::InMemorySessionStore;

    async fn app_with(perms: &str) -> (Router, String) {
        let repo = Arc::new(InMemoryVerificationRepository::new());
        let sessions = Arc::new(InMemorySessionStore::new());
        let set = PermissionSet::from_raw([perms.to_string()]).expect("perms");
        let token = sessions.create("u", set, 3600).await.expect("token");
        let r = router(
            CreateVerificationUseCase::new(repo.clone()),
            VerificationService::new(repo),
            sessions,
        );
        (r, token)
    }

    fn req(method: &str, uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method(method).uri(uri).header("content-type", "application/json");
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
    async fn full_traceability_flow_to_complete() {
        let (app, t) = app_with("VERIFICATION:READ+ADD+UPDATE").await;
        // create with 2 criteria
        let r = app
            .clone()
            .oneshot(req("POST", "/verification", r#"{"requirementId":"req1","requirementVersion":1,"criteria":["登录成功","错误密码拒绝"]}"#, Some(&t)))
            .await
            .expect("r");
        assert_eq!(r.status(), StatusCode::CREATED);
        let id = json(r).await["id"].as_str().expect("id").to_string();

        // report: 2 uncovered gaps
        let rep = app.clone().oneshot(req("GET", &format!("/verification/{id}/report"), "", Some(&t))).await.expect("r");
        let v = json(rep).await;
        assert_eq!(v["complete"], false);
        assert_eq!(v["gaps"].as_array().expect("a").len(), 2);
        assert_eq!(v["gaps"][0]["kind"], "UNCOVERED");

        // link both criteria
        app.clone().oneshot(req("POST", &format!("/verification/{id}/link"), r#"{"criterionIndex":0,"decompositionId":"d1","taskId":"t1"}"#, Some(&t))).await.expect("r");
        app.clone().oneshot(req("POST", &format!("/verification/{id}/link"), r#"{"criterionIndex":1,"decompositionId":"d1","taskId":"t2"}"#, Some(&t))).await.expect("r");
        // now gaps are UNVERIFIED
        let rep = app.clone().oneshot(req("GET", &format!("/verification/{id}/report"), "", Some(&t))).await.expect("r");
        assert_eq!(json(rep).await["gaps"][0]["kind"], "UNVERIFIED");

        // sync both verified
        app.clone().oneshot(req("POST", &format!("/verification/{id}/sync"), r#"{"decompositionId":"d1","taskId":"t1","satisfied":true}"#, Some(&t))).await.expect("r");
        let last = app.clone().oneshot(req("POST", &format!("/verification/{id}/sync"), r#"{"decompositionId":"d1","taskId":"t2","satisfied":true}"#, Some(&t))).await.expect("r");
        assert_eq!(json(last).await["complete"], true);

        let rep = app.oneshot(req("GET", &format!("/verification/{id}/report"), "", Some(&t))).await.expect("r");
        let v = json(rep).await;
        assert_eq!(v["complete"], true);
        assert_eq!(v["satisfied"], 2);
        assert_eq!(v["gaps"].as_array().expect("a").len(), 0);
    }

    #[tokio::test]
    async fn rbac_create_and_link() {
        // 无令牌 → 401
        let (app, _t) = app_with("VERIFICATION:READ").await;
        assert_eq!(
            app.oneshot(req("POST", "/verification", r#"{"requirementId":"r","requirementVersion":1,"criteria":["c"]}"#, None)).await.expect("r").status(),
            StatusCode::UNAUTHORIZED
        );
        // 只读 → 403
        let (app, t) = app_with("VERIFICATION:READ").await;
        assert_eq!(
            app.oneshot(req("POST", "/verification", r#"{"requirementId":"r","requirementVersion":1,"criteria":["c"]}"#, Some(&t))).await.expect("r").status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn link_unknown_criterion_404() {
        let (app, t) = app_with("VERIFICATION:READ+ADD+UPDATE").await;
        let r = app.clone().oneshot(req("POST", "/verification", r#"{"requirementId":"r","requirementVersion":1,"criteria":["c"]}"#, Some(&t))).await.expect("r");
        let id = json(r).await["id"].as_str().expect("id").to_string();
        assert_eq!(
            app.oneshot(req("POST", &format!("/verification/{id}/link"), r#"{"criterionIndex":9,"decompositionId":"d","taskId":"t"}"#, Some(&t))).await.expect("r").status(),
            StatusCode::NOT_FOUND
        );
    }
}
