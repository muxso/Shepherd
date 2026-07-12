use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::{
    extract::{FromRef, FromRequestParts, State},
    http::{header::ACCEPT, request::Parts, HeaderValue, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt;

use mcp::{CapabilityChecker, McpServer, Tool, ToolHandler};
use tokio_stream::wrappers::BroadcastStream;
use webauth::{AuthUser, SessionStore};

use crate::mcp_bus::McpBus;

struct UserCaps<'a>(&'a AuthUser);
impl CapabilityChecker for UserCaps<'_> {
    fn allows(&self, resource: &str, action: &str) -> bool {
        self.0.can(resource, action)
    }
}

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

const SESSION_HEADER: &str = "mcp-session-id";

// Sessions are bound to the authenticated user: ids are random and unguessable, and
// only the owner (same Bearer user) may reuse/delete one — prevents authenticated
// user A from manipulating B's session via a guessable id (defense in depth; /mcp
// already requires Bearer auth).
#[derive(Default)]
struct McpSessions {
    owners: Mutex<HashMap<String, String>>,
}

impl McpSessions {
    fn mint(&self, owner: &str) -> String {
        let id = format!("mcp-{}", uuid::Uuid::new_v4().simple());
        self.owners
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id.clone(), owner.to_string());
        id
    }
    fn owns(&self, id: &str, owner: &str) -> bool {
        self.owners
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(id)
            .map(|o| o == owner)
            .unwrap_or(false)
    }
    fn remove(&self, id: &str, owner: &str) -> bool {
        let mut g = self.owners.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if g.get(id).map(|o| o == owner).unwrap_or(false) {
            g.remove(id);
            true
        } else {
            false
        }
    }
}

struct SessionHeader(Option<String>);

impl<S: Send + Sync> FromRequestParts<S> for SessionHeader {
    type Rejection = Infallible;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(SessionHeader(
            parts.headers.get(SESSION_HEADER).and_then(|v| v.to_str().ok()).map(String::from),
        ))
    }
}

use delivery::application::DeliveryService;
use requirement::application::{CreateRequirementUseCase, RequirementCmdError, RequirementService};
use skill::application::{CreateSkillUseCase, SkillService};
use task::application::{BreakdownUseCase, CreateDecompositionUseCase, TaskService};
use task::ports::RequirementSpec;
use verification::application::{CreateVerificationUseCase, VerificationService};

use crate::plan_run::PlanRunner;
use probe::{ProbeAssertion, ProbeRequest};
use runner::application::RunnerService;
use test_plan::application::{CreatePlanUseCase, PlanCaseUseCase, PlanStatisticsUseCase};
use test_plan::domain::{PlanType, ROOT_GROUP};

fn req_str<'a>(v: &'a Value, k: &str) -> Result<&'a str, String> {
    v.get(k).and_then(|x| x.as_str()).ok_or_else(|| format!("'{k}' (string) is required"))
}
fn opt_str<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}
fn req_u32(v: &Value, k: &str) -> Result<u32, String> {
    v.get(k)
        .and_then(|x| x.as_u64())
        .map(|n| n as u32)
        .ok_or_else(|| format!("'{k}' (number) is required"))
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
        .execute(
            req_str(&args, "projectId")?,
            req_str(&args, "title")?,
            opt_str(&args, "description"),
            &str_vec(&args, "acceptanceCriteria"),
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(
        json!({ "id": r.id, "title": r.title, "baselineVersion": r.baseline_version, "latestVersion": r.latest_version() }),
    )
});

tool_handler!(Decompose, CreateDecompositionUseCase, |self, args| {
    let d = self
        .svc
        .execute(req_str(&args, "requirementId")?, req_u32(&args, "requirementVersion")?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(
        json!({ "decompositionId": d.id, "requirementId": d.requirement_id, "requirementVersion": d.requirement_version }),
    )
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
            args.get("points").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!({ "taskId": id }))
});

struct DispatchDelivery {
    delivery: DeliveryService,
    skills: SkillService,
}

#[async_trait]
impl ToolHandler for DispatchDelivery {
    async fn call(&self, args: Value) -> Result<Value, String> {
        let mut instructions = args.get("instructions").and_then(|x| x.as_str()).map(String::from);
        let skill_ids = str_vec(&args, "skillIds");
        if !skill_ids.is_empty() {
            let project = req_str(&args, "projectId")?;
            let comp =
                self.skills.compose(project, &skill_ids).await.map_err(|e| format!("{e:?}"))?;
            instructions = Some(match instructions {
                Some(extra) if !extra.trim().is_empty() => {
                    format!("{}\n\n{}", comp.instructions, extra)
                }
                _ => comp.instructions,
            });
        }
        let a = self
            .delivery
            .dispatch(
                req_str(&args, "decompositionId")?,
                req_str(&args, "taskId")?,
                req_str(&args, "title")?,
                opt_str(&args, "description"),
                &str_vec(&args, "acceptanceCriteria"),
                req_str(&args, "executor")?,
                None,
                instructions,
                None,
            )
            .await
            .map_err(|e| format!("{e:?}"))?;
        Ok(json!({
            "attemptId": a.id,
            "status": a.status.as_str(),
            "deliverable": a.deliverable.as_ref().map(|d| json!({ "kind": d.kind.as_str(), "reference": d.reference }))
        }))
    }
}

struct Breakdown {
    requirements: RequirementService,
    breakdown: BreakdownUseCase,
}

#[async_trait]
impl ToolHandler for Breakdown {
    async fn call(&self, args: Value) -> Result<Value, String> {
        let id = req_str(&args, "requirementId")?;
        let req = match self.requirements.get(id).await {
            Ok(r) => r,
            Err(RequirementCmdError::NotFound) => return Err("requirement not found".into()),
            Err(e) => return Err(format!("{e:?}")),
        };
        let version = args
            .get("requirementVersion")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .unwrap_or(req.baseline_version);
        let ver = req.version(version).ok_or("requirement version not found")?;
        let spec = RequirementSpec {
            requirement_id: req.id.clone(),
            requirement_version: version,
            title: req.title.clone(),
            description: ver.description.clone(),
            acceptance_criteria: ver.acceptance_criteria.iter().map(|c| c.text.clone()).collect(),
        };
        let d = self.breakdown.execute(&spec).await.map_err(|e| format!("{e:?}"))?;
        Ok(json!({
            "decompositionId": d.id,
            "tasks": d.tasks.iter().map(|t| json!({"id": t.id, "title": t.title, "dependencies": t.dependencies})).collect::<Vec<_>>()
        }))
    }
}

tool_handler!(CreateVerification, CreateVerificationUseCase, |self, args| {
    let v = self
        .svc
        .execute(
            req_str(&args, "requirementId")?,
            req_u32(&args, "requirementVersion")?,
            &str_vec(&args, "criteria"),
        )
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
    let r =
        self.svc.report(req_str(&args, "verificationId")?).await.map_err(|e| format!("{e:?}"))?;
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
    mcp_sessions: Arc<McpSessions>,
    bus: McpBus,
}

impl FromRef<McpState> for Arc<dyn SessionStore> {
    fn from_ref(s: &McpState) -> Self {
        s.sessions.clone()
    }
}

tool_handler!(CreateTestPlan, CreatePlanUseCase, |self, args| {
    let pt = PlanType::parse(args.get("type").and_then(|x| x.as_str()).unwrap_or("TEST_PLAN"))
        .unwrap_or(PlanType::Plan);
    let p = self
        .svc
        .execute(req_str(&args, "projectId")?, req_str(&args, "name")?, pt, ROOT_GROUP)
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!({ "id": p.id, "name": p.name }))
});

tool_handler!(LinkPlanCase, PlanCaseUseCase, |self, args| {
    self.svc
        .link(req_str(&args, "planId")?, req_str(&args, "caseId")?, opt_str(&args, "name"))
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!({ "linked": true }))
});

tool_handler!(TestPlanStats, PlanStatisticsUseCase, |self, args| {
    let s = self.svc.execute(req_str(&args, "planId")?).await.map_err(|e| format!("{e:?}"))?;
    Ok(json!({
        "status": s.status.as_str(), "total": s.total,
        "passRate": s.pass_rate, "executeRate": s.execute_rate, "isPass": s.is_pass
    }))
});

struct RunTestPlan {
    runner: PlanRunner,
}
#[async_trait]
impl ToolHandler for RunTestPlan {
    async fn call(&self, args: Value) -> Result<Value, String> {
        let s = self
            .runner
            .run(req_str(&args, "planId")?, args.get("environmentId").and_then(|x| x.as_str()))
            .await
            .map_err(|()| "plan execute failed".to_string())?;
        Ok(json!({
            "total": s.total, "executed": s.executed, "success": s.success, "failed": s.failed
        }))
    }
}

struct ProbeTool {
    runner: RunnerService,
}
#[async_trait]
impl ToolHandler for ProbeTool {
    async fn call(&self, args: Value) -> Result<Value, String> {
        let assertions: Vec<ProbeAssertion> = args
            .get("assertions")
            .cloned()
            .map(|v| serde_json::from_value(v).unwrap_or_default())
            .unwrap_or_default();
        let metadata = args
            .get("metadata")
            .and_then(|m| m.as_object())
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let req = ProbeRequest {
            protocol: req_str(&args, "protocol")?.to_string(),
            target: req_str(&args, "target")?.to_string(),
            payload: args.get("payload").and_then(|x| x.as_str()).map(String::from),
            metadata,
            assertions,
        };
        let rep = self.runner.run_probe(&req).await.map_err(|e| format!("{e:?}"))?;
        Ok(json!({
            "agentId": rep.agent_id, "agentName": rep.agent_name, "outcome": rep.outcome
        }))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn router(
    create_requirement: CreateRequirementUseCase,
    decompose: CreateDecompositionUseCase,
    tasks: TaskService,
    delivery: DeliveryService,
    requirements: RequirementService,
    breakdown: BreakdownUseCase,
    create_verification: CreateVerificationUseCase,
    verification: VerificationService,
    create_skill: CreateSkillUseCase,
    skills: SkillService,
    create_plan: CreatePlanUseCase,
    plan_cases: PlanCaseUseCase,
    plan_stats: PlanStatisticsUseCase,
    plan_runner: PlanRunner,
    runner_svc: RunnerService,
    sessions: Arc<dyn SessionStore>,
    bus: McpBus,
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
            "把任务派发给 AI 执行者(executor: CLAUDE_CODE | CODEX | OPENCODE | CODEBUDDY);自动驱动任务、过验证门并回灌验证。\
             可直接传 skillIds(+projectId)自动组合行为规范注入,无需先调 compose。",
            obj(
                json!({
                    "decompositionId": { "type": "string" },
                    "taskId": { "type": "string" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "acceptanceCriteria": { "type": "array", "items": { "type": "string" } },
                    "executor": { "type": "string", "enum": ["CLAUDE_CODE", "CODEX", "OPENCODE", "CODEBUDDY"] },
                    "projectId": { "type": "string", "description": "skillIds 非空时必填(用于组合)" },
                    "skillIds": { "type": "array", "items": { "type": "string" }, "description": "可选:自动组合这些 skill 为行为规范" },
                    "instructions": { "type": "string", "description": "可选:显式行为规范(与 skillIds 组合结果合并)" }
                }),
                &["decompositionId", "taskId", "title", "executor"],
            ),
            Arc::new(DispatchDelivery { delivery, skills: skills.clone() }),
        )
        .requires("DELIVERY", "EXECUTE"))
        .tool(Tool::new(
            "shepherd_breakdown",
            "自动拆分:仅需 requirementId(可选 requirementVersion),服务端取需求规格并用规划器\
             (默认启发式,可配 LLM)生成任务 DAG。",
            obj(
                json!({
                    "requirementId": { "type": "string" },
                    "requirementVersion": { "type": "integer", "description": "可选,默认基线版本" }
                }),
                &["requirementId"],
            ),
            Arc::new(Breakdown { requirements, breakdown }),
        )
        .requires("TASK", "ADD"))
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
        .requires("VERIFICATION", "READ"))
        .tool(Tool::new(
            "shepherd_create_test_plan",
            "创建测试计划(type: TEST_PLAN 默认 | GROUP),返回计划 id。",
            obj(
                json!({
                    "projectId": { "type": "string" },
                    "name": { "type": "string" },
                    "type": { "type": "string", "enum": ["TEST_PLAN", "GROUP"] }
                }),
                &["projectId", "name"],
            ),
            Arc::new(CreateTestPlan { svc: create_plan }),
        )
        .requires("TEST_PLAN", "ADD"))
        .tool(Tool::new(
            "shepherd_link_plan_case",
            "把一条用例或场景挂入计划(caseId 为 ms_api_case id 或场景 id)。",
            obj(
                json!({
                    "planId": { "type": "string" },
                    "caseId": { "type": "string" },
                    "name": { "type": "string", "description": "展示名(可选)" }
                }),
                &["planId", "caseId"],
            ),
            Arc::new(LinkPlanCase { svc: plan_cases }),
        )
        .requires("TEST_PLAN", "ADD"))
        .tool(Tool::new(
            "shepherd_run_test_plan",
            "执行计划:跑挂入的用例/场景并自动回写结果(随后报告即真实数据)。可传 environmentId 注入鉴权头。返回执行汇总。",
            obj(json!({ "planId": { "type": "string" }, "environmentId": { "type": "string", "description": "可选:注入 base_url + 默认头(如 Authorization)" } }), &["planId"]),
            Arc::new(RunTestPlan { runner: plan_runner }),
        )
        .requires("TEST_PLAN", "EXECUTE"))
        .tool(Tool::new(
            "shepherd_test_plan_stats",
            "取计划执行统计:状态/总数/通过率/执行率/是否通过。",
            obj(json!({ "planId": { "type": "string" } }), &["planId"]),
            Arc::new(TestPlanStats { svc: plan_stats }),
        )
        .requires("TEST_PLAN", "READ"))
        .tool(Tool::new(
            "shepherd_probe",
            "按协议探测:中央据 protocol(http/grpc/sql/mysql/redis/websocket)选支持它的 runner-agent \
             就地执行,可带断言。返回选中 agent + 判定结果。",
            obj(
                json!({
                    "protocol": { "type": "string" },
                    "target": { "type": "string", "description": "URL / 端点 / 连接串" },
                    "payload": { "type": "string", "description": "可选载荷(body/语句/命令/消息)" },
                    "metadata": { "type": "object", "description": "可选协议参数(如 gRPC method)" },
                    "assertions": { "type": "array", "items": { "type": "object" }, "description": "可选 probe 断言数组" }
                }),
                &["protocol", "target"],
            ),
            Arc::new(ProbeTool { runner: runner_svc }),
        )
        .requires("RUNNER", "EXECUTE"));

    Router::new().route("/mcp", post(mcp_handler).get(mcp_sse).delete(mcp_delete)).with_state(
        McpState {
            server: Arc::new(server),
            sessions,
            mcp_sessions: Arc::new(McpSessions::default()),
            bus,
        },
    )
}

fn is_initialize(body: &Value) -> bool {
    body.get("method").and_then(|m| m.as_str()) == Some("initialize")
}

async fn mcp_handler(
    user: AuthUser,
    WantsSse(wants_sse): WantsSse,
    SessionHeader(sid): SessionHeader,
    State(st): State<McpState>,
    Json(body): Json<Value>,
) -> Response {
    if let Some(id) = &sid {
        if !st.mcp_sessions.owns(id, &user.user_id) {
            return (StatusCode::NOT_FOUND, "unknown session").into_response();
        }
    }
    let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let tool = if method == "tools/call" {
        body.pointer("/params/name").and_then(|n| n.as_str()).unwrap_or("").to_string()
    } else {
        String::new()
    };
    let mint = is_initialize(&body);
    let resp = st.server.dispatch(body, &UserCaps(&user)).await;
    let errored = resp.as_ref().map(|r| r.get("error").is_some()).unwrap_or(false);
    tracing::info!(target: "mcp_audit", user = %user.user_id, method, tool, ok = !errored, "MCP 调用");
    let new_sid = if mint { Some(st.mcp_sessions.mint(&user.user_id)) } else { None };

    let mut response = match resp {
        None => StatusCode::ACCEPTED.into_response(),
        Some(resp) if wants_sse => {
            let body = format!(
                "event: message\ndata: {}\n\n",
                serde_json::to_string(&resp).unwrap_or_default()
            );
            Response::builder()
                .status(StatusCode::OK)
                .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
                .header(axum::http::header::CACHE_CONTROL, "no-cache")
                .body(axum::body::Body::from(body))
                .expect("sse response")
        }
        Some(resp) => (StatusCode::OK, Json(resp)).into_response(),
    };
    if let Some(id) = new_sid {
        if let Ok(v) = HeaderValue::from_str(&id) {
            response.headers_mut().insert(SESSION_HEADER, v);
        }
    }
    response
}

async fn mcp_sse(
    user: AuthUser,
    SessionHeader(sid): SessionHeader,
    State(st): State<McpState>,
) -> Response {
    if let Some(id) = &sid {
        if !st.mcp_sessions.owns(id, &user.user_id) {
            return (StatusCode::NOT_FOUND, "unknown session").into_response();
        }
    }
    let ready = tokio_stream::once(Ok::<Event, Infallible>(
        Event::default().event("ready").data(json!({ "server": "shepherd" }).to_string()),
    ));
    // Server push: subscribe to the event bus and turn task/delivery/verification
    // events into SSE notifications; lagged entries are skipped.
    let notifications = BroadcastStream::new(st.bus.subscribe()).filter_map(|r| {
        r.ok().map(|ev| {
            Ok::<Event, Infallible>(
                Event::default()
                    .event("notification")
                    .data(serde_json::to_string(&ev).unwrap_or_default()),
            )
        })
    });
    let beats = IntervalStream::new(tokio::time::interval(Duration::from_secs(15)))
        .map(|_| Ok::<Event, Infallible>(Event::default().event("heartbeat").data("ping")));
    Sse::new(ready.chain(notifications.merge(beats)))
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn mcp_delete(
    user: AuthUser,
    SessionHeader(sid): SessionHeader,
    State(st): State<McpState>,
) -> Response {
    match sid {
        Some(id) if st.mcp_sessions.remove(&id, &user.user_id) => {
            StatusCode::NO_CONTENT.into_response()
        }
        Some(_) => (StatusCode::NOT_FOUND, "unknown session").into_response(),
        None => (StatusCode::BAD_REQUEST, "missing Mcp-Session-Id").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::McpSessions;

    #[test]
    fn sessions_are_unguessable_and_owner_scoped() {
        let s = McpSessions::default();
        let a = s.mint("alice");
        let b = s.mint("bob");
        assert_ne!(a, b);
        assert!(a.starts_with("mcp-") && a.len() >= 36, "random id, not a counter: {a}");

        assert!(s.owns(&a, "alice"));
        assert!(!s.owns(&a, "bob"), "another user must not see alice's session");
        assert!(!s.owns("mcp-guessed", "alice"));

        assert!(!s.remove(&a, "bob"), "cross-user delete must be refused");
        assert!(s.owns(&a, "alice"), "refused delete must not touch the session");
        assert!(s.remove(&a, "alice"));
        assert!(!s.owns(&a, "alice"));

        assert!(s.owns(&b, "bob"), "bob's session untouched throughout");
    }
}
