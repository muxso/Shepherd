use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use webauth::{AuthUser, SessionStore};

use case_management::domain::NewFunctionalCase;
use case_management::ports::CaseRepository;
use requirement::application::{RequirementCmdError, RequirementService};
use task::application::{BreakdownError, BreakdownUseCase};
use task::ports::RequirementSpec;
use verification::application::{CreateVerificationError, CreateVerificationUseCase};

#[derive(Clone)]
struct BreakdownState {
    reqs: RequirementService,
    breakdown: BreakdownUseCase,
    create_verification: CreateVerificationUseCase,
    cases: Arc<dyn CaseRepository>,
    sessions: Arc<dyn SessionStore>,
}

async fn seed_functional_cases(
    cases: &Arc<dyn CaseRepository>,
    requirement_id: &str,
    project_id: &str,
    criteria: &[String],
    created_by: &str,
) {
    if criteria.is_empty() {
        return;
    }
    match cases.cases_for_requirement(requirement_id).await {
        Ok(existing) if !existing.is_empty() => return,
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(requirement = %requirement_id, "拆分后查功能用例覆盖失败: {e:?}");
            return;
        }
    }
    for (idx, text) in criteria.iter().enumerate() {
        let new = match NewFunctionalCase::new(
            project_id,
            text,
            "需求拆分",
            "",
            "",
            std::collections::BTreeMap::new(),
            Vec::new(),
        ) {
            Ok(n) => n.with_created_by(Some(created_by)),
            Err(e) => {
                tracing::warn!(requirement = %requirement_id, "生成功能用例(标准{idx})失败: {e:?}");
                continue;
            }
        };
        match cases.insert(&new).await {
            Ok(c) => {
                if let Err(e) =
                    cases.link_requirement_case(requirement_id, idx as i32, &c.id, project_id).await
                {
                    tracing::warn!(requirement = %requirement_id, "关联功能用例(标准{idx})失败: {e:?}");
                }
            }
            Err(e) => {
                tracing::warn!(requirement = %requirement_id, "插入功能用例(标准{idx})失败: {e:?}")
            }
        }
    }
}

impl FromRef<BreakdownState> for Arc<dyn SessionStore> {
    fn from_ref(s: &BreakdownState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(
    reqs: RequirementService,
    breakdown: BreakdownUseCase,
    create_verification: CreateVerificationUseCase,
    cases: Arc<dyn CaseRepository>,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/requirement/{id}/breakdown", post(breakdown_handler))
        .with_state(BreakdownState { reqs, breakdown, create_verification, cases, sessions })
}

#[derive(Deserialize)]
struct VersionQuery {
    version: Option<u32>,
}

async fn breakdown_handler(
    user: AuthUser,
    State(st): State<BreakdownState>,
    Path(id): Path<String>,
    Query(q): Query<VersionQuery>,
) -> Response {
    if !user.can("TASK", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let req = match st.reqs.get(&id).await {
        Ok(r) => r,
        Err(RequirementCmdError::NotFound) => {
            return (StatusCode::NOT_FOUND, "requirement not found").into_response();
        }
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    };
    let version = q.version.unwrap_or(req.baseline_version);
    let Some(ver) = req.version(version) else {
        return (StatusCode::NOT_FOUND, "requirement version not found").into_response();
    };
    let spec = RequirementSpec {
        requirement_id: req.id.clone(),
        requirement_version: version,
        title: req.title.clone(),
        description: ver.description.clone(),
        acceptance_criteria: ver.acceptance_criteria.iter().map(|c| c.text.clone()).collect(),
    };

    match st.breakdown.execute(&spec).await {
        Ok(d) => {
            let verification_id = if spec.acceptance_criteria.is_empty() {
                None
            } else {
                match st
                    .create_verification
                    .execute(&spec.requirement_id, version, &spec.acceptance_criteria)
                    .await
                {
                    Ok(v) => Some(v.id),
                    Err(CreateVerificationError::AlreadyExists) => None,
                    Err(e) => {
                        tracing::warn!(requirement = %spec.requirement_id, version, "breakdown 后自动开验证失败: {e:?}");
                        None
                    }
                }
            };
            seed_functional_cases(
                &st.cases,
                &spec.requirement_id,
                &req.project_id,
                &spec.acceptance_criteria,
                &user.user_id,
            )
            .await;
            let body = json!({
                "id": d.id,
                "requirementId": d.requirement_id,
                "requirementVersion": d.requirement_version,
                "complete": d.is_complete(),
                "readyTaskIds": d.ready_tasks().iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
                "verificationId": verification_id,
                "tasks": d.tasks.iter().map(|t| json!({
                    "id": t.id, "title": t.title, "status": t.status.as_str(), "dependencies": t.dependencies
                })).collect::<Vec<_>>()
            });
            (StatusCode::CREATED, Json(body)).into_response()
        }
        Err(BreakdownError::AlreadyExists) => {
            match st.breakdown.find_existing(&spec.requirement_id, version).await {
                Ok(Some(d)) => {
                    let verification_id = st
                        .create_verification
                        .find_existing(&spec.requirement_id, version)
                        .await
                        .ok()
                        .flatten()
                        .map(|v| v.id);
                    seed_functional_cases(
                        &st.cases,
                        &spec.requirement_id,
                        &req.project_id,
                        &spec.acceptance_criteria,
                        &user.user_id,
                    )
                    .await;
                    let body = json!({
                        "id": d.id,
                        "requirementId": d.requirement_id,
                        "requirementVersion": d.requirement_version,
                        "complete": d.is_complete(),
                        "readyTaskIds": d.ready_tasks().iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
                        "verificationId": verification_id,
                        "tasks": d.tasks.iter().map(|t| json!({
                            "id": t.id, "title": t.title, "status": t.status.as_str(), "dependencies": t.dependencies
                        })).collect::<Vec<_>>()
                    });
                    (StatusCode::OK, Json(body)).into_response()
                }
                _ => (StatusCode::CONFLICT, "decomposition already exists").into_response(),
            }
        }
        Err(BreakdownError::EmptyRequirement) => {
            (StatusCode::BAD_REQUEST, "requirement id required").into_response()
        }
        Err(BreakdownError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "invalid planned task").into_response()
        }
        Err(BreakdownError::Plan(_)) => (StatusCode::BAD_GATEWAY, "planner error").into_response(),
        Err(BreakdownError::Repo(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
        }
    }
}
