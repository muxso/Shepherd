//! 服务端复合路由 `POST /requirement/{id}/breakdown?version=N`:据 requirementId **服务端取规格**
//! (从需求基线/指定版本读出 title/description/验收标准)再交 task 的 BreakdownUseCase 拆分。
//!
//! 这是 requirement → task 的跨上下文协调,放在组装根;task / requirement 彼此不依赖。
//!
//! 拆分成功后**顺手开完整性验证账本**(同一份验收标准快照,幂等:已存在则跳过)。
//! 配合编排器在交付终态按标准文本自动建覆盖链 + 回灌,`breakdown → dispatch` 即可让
//! 完整性报告自动收敛,无需手工 `verify create` / `verify link`。

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

/// 拆分后为每条验收标准生成一个功能用例草稿并关联(打通功能用例覆盖)。
/// 幂等:该需求已有关联功能用例则整体跳过;尽力而为,失败仅日志,不影响拆分结果。
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
        Ok(existing) if !existing.is_empty() => return, // 已生成过 → 幂等跳过
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
            Err(e) => tracing::warn!(requirement = %requirement_id, "插入功能用例(标准{idx})失败: {e:?}"),
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
    // 服务端取需求规格(默认基线版本)。
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
            // 顺手开验证账本(同一份验收标准快照)。幂等:已存在则跳过;有标准才开。
            // 尽力而为——拆分已落库,开账本失败不应让整个 breakdown 失败(仅日志告警)。
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
            // 顺手为每条验收标准生成功能用例草稿并关联(幂等,尽力而为)。
            seed_functional_cases(&st.cases, &spec.requirement_id, &req.project_id, &spec.acceptance_criteria, &user.user_id).await;
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
            // 幂等:已拆分则回读现有拆分图(含现有验证账本 id)并返回 200,
            // 让前端「自动拆分」可重入(原先重复点击会 409)。
            match st.breakdown.find_existing(&spec.requirement_id, version).await {
                Ok(Some(d)) => {
                    let verification_id = st
                        .create_verification
                        .find_existing(&spec.requirement_id, version)
                        .await
                        .ok()
                        .flatten()
                        .map(|v| v.id);
                    // 幂等补种:旧拆分(本功能上线前)可能还没功能用例覆盖,这里补上。
                    seed_functional_cases(&st.cases, &spec.requirement_id, &req.project_id, &spec.acceptance_criteria, &user.user_id).await;
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
        Err(BreakdownError::EmptyRequirement) => (StatusCode::BAD_REQUEST, "requirement id required").into_response(),
        Err(BreakdownError::Validation(_)) => (StatusCode::BAD_REQUEST, "invalid planned task").into_response(),
        Err(BreakdownError::Plan(_)) => (StatusCode::BAD_GATEWAY, "planner error").into_response(),
        Err(BreakdownError::Repo(_)) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response(),
    }
}
