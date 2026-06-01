//! MCP 工具桥接:把 Shepherd 各上下文服务注册成 MCP 工具,暴露 `POST /mcp`(JSON-RPC)。
//! 让 AI(Claude 等)经 Model Context Protocol 直接驱动「需求→拆任务→派发→验证」全链路。
//!
//! `/mcp` 需有效会话(`AuthUser`,无令牌→401);细粒度按工具 RBAC 留作后续。

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::{FromRef, FromRequestParts, State},
    http::{header::ACCEPT, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};

use mcp::{CapabilityChecker, McpServer, Tool, ToolHandler};
use webauth::{AuthUser, SessionStore};

/// 用会话权限实现 MCP 能力检查(按工具 RBAC)。
struct UserCaps<'a>(&'a AuthUser);
impl CapabilityChecker for UserCaps<'_> {
    fn allows(&self, resource: &str, action: &str) -> bool {
        self.0.can(resource, action)
    }
}

/// 提取器:客户端是否要求 SSE(`Accept: text/event-stream`)。
struct WantsSse(bool);

impl<S: Send + Sync> FromRequestParts<S> for WantsSse {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let sse = parts
            .headers
            .get(ACCEPT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.contains("text/event-stream"))
            .unwrap_or(false);
        Ok(WantsSse(sse))
    }
}

use delivery::application::DeliveryService;
use requirement::application::CreateRequirementUseCase;
use skill::application::{CreateSkillUseCase, SkillService};
use task::application::{CreateDecompositionUseCase, TaskService};
use verification::application::{CreateVerificationUseCase, VerificationService};

// —— 取参助手 ——
fn req_str<'a>(v: &'a Value, k: &str) -> Result<&'a str, String> {
    v.get(k).and_then(|x| x.as_str()).ok_or_else(|| format!("'{k}' (string) is required"))
}
fn opt_str<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}
fn req_u32(v: &Value, k: &str) -> Result<u32, String> {
    v.get(k).and_then(|x| x.as_u64()).map(|n| n as u32).ok_or_else(|| format!("'{k}' (number) is required"))
}
fn str_vec(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

macro_rules! tool_handler {
    ($name:ident, $svc:ty, |$self_:ident, $args:ident| $body:block) => {
        struct $name {
            svc: $svc,
        }
        #[async_trait]
        impl ToolHandler for $name {
            async fn call(&$self_, $args: Value) -> Result<Value, String> $body
        }
    };
}

tool_handler!(CreateRequirement, CreateRequirementUseCase, |self, args| {
    let r = self
        .svc
        .execute(req_str(&args, "projectId")?, req_str(&args, "title")?, opt_str(&args, "description"), &str_vec(&args, "acceptanceCriteria"))
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!({ "id": r.id, "title": r.title, "baselineVersion": r.baseline_version, "latestVersion": r.latest_version() }))
});

tool_handler!(Decompose, CreateDecompositionUseCase, |self, args| {
    let d = self
        .svc
        .execute(req_str(&args, "requirementId")?, req_u32(&args, "requirementVersion")?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!({ "decompositionId": d.id, "requirementId": d.requirement_id, "requirementVersion": d.requirement_version }))
});

tool_handler!(AddTask, TaskService, |self, args| {
    let id = self
        .svc
        .add_task(
            req_str(&args, "decompositionId")?,
            req_str(&args, "title")?,
            opt_str(&args, "description"),
            &str_vec(&args, "acceptanceCriteria"),
            &str_vec(&args, "dependencies"),
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!({ "taskId": id }))
});

tool_handler!(DispatchDelivery, DeliveryService, |self, args| {
    let a = self
        .svc
        .dispatch(
            req_str(&args, "decompositionId")?,
            req_str(&args, "taskId")?,
            req_str(&args, "title")?,
            opt_str(&args, "description"),
            &str_vec(&args, "acceptanceCriteria"),
            req_str(&args, "executor")?,
            None,
            args.get("instructions").and_then(|x| x.as_str()).map(String::from),
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!({
        "attemptId": a.id,
        "status": a.status.as_str(),
        "deliverable": a.deliverable.as_ref().map(|d| json!({ "kind": d.kind.as_str(), "reference": d.reference }))
    }))
});

tool_handler!(CreateVerification, CreateVerificationUseCase, |self, args| {
    let v = self
        .svc
        .execute(req_str(&args, "requirementId")?, req_u32(&args, "requirementVersion")?, &str_vec(&args, "criteria"))
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!({ "verificationId": v.id }))
});

tool_handler!(LinkCoverage, VerificationService, |self, args| {
    self.svc
        .link(
            req_str(&args, "verificationId")?,
            req_u32(&args, "criterionIndex")?,
            req_str(&args, "decompositionId")?,
            req_str(&args, "taskId")?,
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!({ "ok": true }))
});

tool_handler!(CreateSkill, CreateSkillUseCase, |self, args| {
    let s = self
        .svc
        .execute(
            req_str(&args, "projectId")?,
            req_str(&args, "name")?,
            opt_str(&args, "description"),
            req_str(&args, "instructions")?,
            &str_vec(&args, "includes"),
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!({ "skillId": s.id, "name": s.name }))
});

tool_handler!(ComposeSkills, SkillService, |self, args| {
    let c = self
        .svc
        .compose(req_str(&args, "projectId")?, &str_vec(&args, "skillIds"))
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!({ "skillIds": c.skill_ids, "instructions": c.instructions }))
});

tool_handler!(CompletenessReport, VerificationService, |self, args| {
    let r = self.svc.report(req_str(&args, "verificationId")?).await.map_err(|e| format!("{e:?}"))?;
    Ok(json!({
        "complete": r.complete,
        "satisfied": r.satisfied,
        "total": r.total,
        "gaps": r.gaps.iter().map(|g| json!({ "criterionIndex": g.criterion_index, "text": g.text, "kind": g.kind.as_str() })).collect::<Vec<_>>()
    }))
});

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

#[derive(Clone)]
struct McpState {
    server: Arc<McpServer>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<McpState> for Arc<dyn SessionStore> {
    fn from_ref(s: &McpState) -> Self {
        s.sessions.clone()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn router(
    create_requirement: CreateRequirementUseCase,
    decompose: CreateDecompositionUseCase,
    tasks: TaskService,
    delivery: DeliveryService,
    create_verification: CreateVerificationUseCase,
    verification: VerificationService,
    create_skill: CreateSkillUseCase,
    skills: SkillService,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    let server = McpServer::new("shepherd", env!("CARGO_PKG_VERSION"))
        .tool(Tool::new(
            "shepherd_create_requirement",
            "创建一个需求(初版 v1),返回需求 id。",
            obj(
                json!({
                    "projectId": { "type": "string" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "acceptanceCriteria": { "type": "array", "items": { "type": "string" } }
                }),
                &["projectId", "title"],
            ),
            Arc::new(CreateRequirement { svc: create_requirement }),
        )
        .requires("REQUIREMENT", "ADD"))
        .tool(Tool::new(
            "shepherd_decompose",
            "为某需求版本开启任务拆分图,返回 decompositionId。",
            obj(
                json!({ "requirementId": { "type": "string" }, "requirementVersion": { "type": "integer" } }),
                &["requirementId", "requirementVersion"],
            ),
            Arc::new(Decompose { svc: decompose }),
        )
        .requires("TASK", "ADD"))
        .tool(Tool::new(
            "shepherd_add_task",
            "向拆分图加入一个任务(可声明 dependencies 为已存在任务的本地 id),返回 taskId。",
            obj(
                json!({
                    "decompositionId": { "type": "string" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "acceptanceCriteria": { "type": "array", "items": { "type": "string" } },
                    "dependencies": { "type": "array", "items": { "type": "string" } }
                }),
                &["decompositionId", "title"],
            ),
            Arc::new(AddTask { svc: tasks }),
        )
        .requires("TASK", "ADD"))
        .tool(Tool::new(
            "shepherd_dispatch_delivery",
            "把一个任务派发给 AI 执行者(executor: CLAUDE_CODE | CODEX);自动驱动任务并回灌验证。",
            obj(
                json!({
                    "decompositionId": { "type": "string" },
                    "taskId": { "type": "string" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "acceptanceCriteria": { "type": "array", "items": { "type": "string" } },
                    "executor": { "type": "string", "enum": ["CLAUDE_CODE", "CODEX"] },
                    "instructions": { "type": "string", "description": "可选:由 shepherd_compose_skills 得到的行为规范" }
                }),
                &["decompositionId", "taskId", "title", "executor"],
            ),
            Arc::new(DispatchDelivery { svc: delivery }),
        )
        .requires("DELIVERY", "EXECUTE"))
        .tool(Tool::new(
            "shepherd_create_verification",
            "为某需求版本开启完整性验证(传入验收标准快照),返回 verificationId。",
            obj(
                json!({
                    "requirementId": { "type": "string" },
                    "requirementVersion": { "type": "integer" },
                    "criteria": { "type": "array", "items": { "type": "string" } }
                }),
                &["requirementId", "requirementVersion"],
            ),
            Arc::new(CreateVerification { svc: create_verification }),
        )
        .requires("VERIFICATION", "ADD"))
        .tool(Tool::new(
            "shepherd_link_coverage",
            "建立覆盖追溯:某任务覆盖某条验收标准(criterionIndex 为 0-based)。",
            obj(
                json!({
                    "verificationId": { "type": "string" },
                    "criterionIndex": { "type": "integer" },
                    "decompositionId": { "type": "string" },
                    "taskId": { "type": "string" }
                }),
                &["verificationId", "criterionIndex", "decompositionId", "taskId"],
            ),
            Arc::new(LinkCoverage { svc: verification.clone() }),
        )
        .requires("VERIFICATION", "UPDATE"))
        .tool(Tool::new(
            "shepherd_create_skill",
            "定义一个可复用的 AI Skill(行为规范);includes 可组合其它 skill。",
            obj(
                json!({
                    "projectId": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "instructions": { "type": "string" },
                    "includes": { "type": "array", "items": { "type": "string" } }
                }),
                &["projectId", "name", "instructions"],
            ),
            Arc::new(CreateSkill { svc: create_skill }),
        )
        .requires("SKILL", "ADD"))
        .tool(Tool::new(
            "shepherd_compose_skills",
            "把若干 skill 经 includes 展开组合成一份有序去重的行为规范(instructions),可直接喂给 dispatch。",
            obj(
                json!({
                    "projectId": { "type": "string" },
                    "skillIds": { "type": "array", "items": { "type": "string" } }
                }),
                &["projectId", "skillIds"],
            ),
            Arc::new(ComposeSkills { svc: skills }),
        )
        .requires("SKILL", "READ"))
        .tool(Tool::new(
            "shepherd_completeness_report",
            "获取完整性报告:整体是否完成 + 缺口清单(UNCOVERED / UNVERIFIED)。",
            obj(json!({ "verificationId": { "type": "string" } }), &["verificationId"]),
            Arc::new(CompletenessReport { svc: verification }),
        )
        .requires("VERIFICATION", "READ"));

    Router::new().route("/mcp", post(mcp_handler)).with_state(McpState { server: Arc::new(server), sessions })
}

/// JSON-RPC 入口。需有效会话;按会话权限做**按工具 RBAC**(只暴露/放行有权工具)。
/// 若客户端 `Accept: text/event-stream`,响应以单条 SSE 事件返回(Streamable HTTP 子集)。
async fn mcp_handler(
    user: AuthUser,
    WantsSse(wants_sse): WantsSse,
    State(st): State<McpState>,
    Json(body): Json<Value>,
) -> Response {
    let resp = st.server.dispatch(body, &UserCaps(&user)).await;

    match resp {
        // 通知:无响应体。
        None => StatusCode::ACCEPTED.into_response(),
        Some(resp) if wants_sse => {
            let body = format!("event: message\ndata: {}\n\n", serde_json::to_string(&resp).unwrap_or_default());
            Response::builder()
                .status(StatusCode::OK)
                .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
                .header(axum::http::header::CACHE_CONTROL, "no-cache")
                .body(axum::body::Body::from(body))
                .expect("sse response")
        }
        Some(resp) => (StatusCode::OK, Json(resp)).into_response(),
    }
}
