//! 场景领域模型 + 步骤编排 + 单层打通(flatten)。
//!
//! 场景(scenario)= 一组有序步骤的编排。每个步骤引用三者之一:
//! 内联请求(REQUEST)/ 接口用例(CASE,按 id)/ 子场景(SCENARIO,可嵌套)。
//! 每步有引用模式 ref_mode:引用(REFERENCE,活链接)或复制(COPY,内联快照)。
//! "打通执行"= compile:递归展开子场景,得到一串可运行步骤(各自携带
//! case_id 交给既有 runner,或一个内联请求)。子场景递归需仓储查找,放在应用层;
//! 本文件只做零 IO 的领域规则与单层展开。

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScenarioError {
    #[error("scenario name must not be empty")]
    EmptyName,
    #[error("scenario project_id must not be empty")]
    EmptyProjectId,
    /// 内联请求的 HTTP 方法不在允许集合内。
    #[error("invalid http method: {0}")]
    InvalidMethod(String),
    /// 内联请求 url 为空。
    #[error("request url must not be empty")]
    EmptyUrl,
    /// CASE 步骤缺少 case_id。
    #[error("case step requires a non-empty case_id")]
    EmptyCaseId,
    /// SCENARIO 步骤缺少 scenario_id。
    #[error("scenario step requires a non-empty scenario_id")]
    EmptyScenarioId,
    /// COPY 模式必须携带快照。
    #[error("copy ref_mode requires a snapshot")]
    MissingSnapshot,
    /// 控制器载荷不是 JSON 对象。
    #[error("control step payload must be a json object")]
    InvalidControl,
    /// 编译时检测到子场景递归成环。
    #[error("cycle detected at scenario: {0}")]
    CycleDetected(String),
    /// 编译递归深度超过上限。
    #[error("max recursion depth exceeded: {0}")]
    MaxDepthExceeded(usize),
}

/// 场景状态。默认 Draft(草稿)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScenarioStatus {
    #[default]
    Draft,
    Debugging,
    Completed,
    Deprecated,
}

impl ScenarioStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScenarioStatus::Draft => "DRAFT",
            ScenarioStatus::Debugging => "DEBUGGING",
            ScenarioStatus::Completed => "COMPLETED",
            ScenarioStatus::Deprecated => "DEPRECATED",
        }
    }

    /// 解析状态字符串;未知值回落到 Draft。
    pub fn parse(s: &str) -> ScenarioStatus {
        match s {
            "DEBUGGING" => ScenarioStatus::Debugging,
            "COMPLETED" => ScenarioStatus::Completed,
            "DEPRECATED" => ScenarioStatus::Deprecated,
            _ => ScenarioStatus::Draft,
        }
    }
}

/// 场景执行状态。默认 Pending(待执行)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionStatus {
    #[default]
    Pending,
    Running,
    Success,
    Error,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionStatus::Pending => "PENDING",
            ExecutionStatus::Running => "RUNNING",
            ExecutionStatus::Success => "SUCCESS",
            ExecutionStatus::Error => "ERROR",
        }
    }

    /// 解析状态字符串;未知值返回 None(与 ScenarioStatus 的回落不同,执行状态需精确)。
    pub fn parse(s: &str) -> Option<ExecutionStatus> {
        match s {
            "PENDING" => Some(ExecutionStatus::Pending),
            "RUNNING" => Some(ExecutionStatus::Running),
            "SUCCESS" => Some(ExecutionStatus::Success),
            "ERROR" => Some(ExecutionStatus::Error),
            _ => None,
        }
    }
}

/// 场景执行记录。每次场景运行落一条:记录状态、用例数与报告 id。
/// created_at 为 RFC3339 字符串(纯领域不引入时间库依赖)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioExecution {
    pub id: String,
    pub scenario_id: String,
    pub project_id: String,
    pub status: ExecutionStatus,
    pub case_count: i32,
    pub report_id: Option<String>,
    pub created_at: String,
}

/// 步骤引用模式。默认 Reference(引用)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RefMode {
    #[default]
    Reference,
    Copy,
}

impl RefMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RefMode::Reference => "REFERENCE",
            RefMode::Copy => "COPY",
        }
    }

    /// 解析引用模式字符串;未知值回落到 Reference。
    pub fn parse(s: &str) -> RefMode {
        match s {
            "COPY" => RefMode::Copy,
            _ => RefMode::Reference,
        }
    }
}

/// 内联请求。method 必须在允许集合内,url 非空。
/// `assertions` 为**中立 JSON 数组**(api-runner Assertion 的序列化形式),组装根执行时再解析为
/// 具体断言——与 ms_api_case 的 assertions 同构,保持 api-scenario 与 api-runner 解耦。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineRequest {
    pub method: String,
    pub url: String,
    pub body: Option<String>,
    pub assertions: serde_json::Value,
}

const ALLOWED_METHODS: &[&str] =
    &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

impl InlineRequest {
    /// 校验构造:method 须在允许集合,url 非空。断言默认空数组(可链式 [`with_assertions`])。
    pub fn new(
        method: &str,
        url: &str,
        body: Option<String>,
    ) -> Result<Self, ScenarioError> {
        let method = method.trim().to_uppercase();
        if !ALLOWED_METHODS.contains(&method.as_str()) {
            return Err(ScenarioError::InvalidMethod(method));
        }
        let url = url.trim();
        if url.is_empty() {
            return Err(ScenarioError::EmptyUrl);
        }
        Ok(Self { method, url: url.to_string(), body, assertions: serde_json::Value::Array(vec![]) })
    }

    /// 附加断言(中立 JSON 数组)。非数组值归一为空数组,避免下游误判。
    pub fn with_assertions(mut self, assertions: serde_json::Value) -> Self {
        self.assertions =
            if assertions.is_array() { assertions } else { serde_json::Value::Array(vec![]) };
        self
    }
}

/// 逻辑控制器类型(对应前端 ScenarioStepType 的控制器子集)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    /// 循环控制器(定次)。
    Loop,
    /// 条件控制器。
    If,
    /// 仅一次控制器。
    Once,
    /// 等待控制器。
    Timer,
}

impl ControlKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ControlKind::Loop => "LOOP",
            ControlKind::If => "IF",
            ControlKind::Once => "ONCE",
            ControlKind::Timer => "TIMER",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "LOOP" => Some(ControlKind::Loop),
            "IF" => Some(ControlKind::If),
            "ONCE" => Some(ControlKind::Once),
            "TIMER" => Some(ControlKind::Timer),
            _ => None,
        }
    }
}

/// 步骤类型:内联请求 / 引用接口用例 / 引用子场景 / 逻辑控制器。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepKind {
    /// 内联请求。
    Request(InlineRequest),
    /// 引用接口用例。
    Case { case_id: String },
    /// 引用子场景(可嵌套)。
    Scenario { scenario_id: String },
    /// 逻辑控制器:类型 + 自包含 JSON 载荷(含子步骤)。落库 kind=控制器类型,inline=载荷。
    Control { control: ControlKind, payload: serde_json::Value },
}

impl StepKind {
    /// 步骤类型字符串(对应落库 kind 列)。
    pub fn kind_str(&self) -> &'static str {
        match self {
            StepKind::Request(_) => "REQUEST",
            StepKind::Case { .. } => "CASE",
            StepKind::Scenario { .. } => "SCENARIO",
            StepKind::Control { control, .. } => control.as_str(),
        }
    }
}

/// 编译产物的计划树节点(中立表示,组装根再转成执行器节点)。
/// 控制器子步骤本版仅支持叶子(CASE/REQUEST),不嵌套控制器。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanStep {
    Case(String),
    Request(InlineRequest),
    Loop { times: u32, body: Vec<PlanStep> },
    If { variable: String, operator: String, value: String, body: Vec<PlanStep> },
    Once { body: Vec<PlanStep> },
    Timer { ms: u64 },
}

/// 控制器嵌套深度上限,防止病态深载荷耗尽栈。
const MAX_CONTROL_DEPTH: usize = 10;

/// 把一个子步骤解析为 PlanStep:叶子(CASE/REQUEST)或**嵌套控制器**(LOOP/IF/ONCE/TIMER)。
/// 超过深度上限或无法识别 → None(被跳过)。
fn parse_plan_step(v: &serde_json::Value, depth: usize) -> Option<PlanStep> {
    match v.get("kind").and_then(|k| k.as_str())?.to_uppercase().as_str() {
        "CASE" => Some(PlanStep::Case(v.get("refId")?.as_str()?.to_string())),
        "REQUEST" => {
            let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("GET");
            let url = v.get("url").and_then(|u| u.as_str()).unwrap_or_default();
            let body = v.get("body").and_then(|b| b.as_str()).map(String::from);
            let assertions =
                v.get("assertions").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![]));
            InlineRequest::new(method, url, body)
                .ok()
                .map(|r| r.with_assertions(assertions))
                .map(PlanStep::Request)
        }
        other => match ControlKind::parse(other) {
            // 子步骤本身是控制器:同一对象既带 kind 又是其载荷,递归(深度受限)。
            Some(ck) if depth < MAX_CONTROL_DEPTH => Some(parse_control_inner(ck, v, depth + 1)),
            _ => None,
        },
    }
}

fn parse_control_inner(control: ControlKind, payload: &serde_json::Value, depth: usize) -> PlanStep {
    let body = || -> Vec<PlanStep> {
        payload
            .get("children")
            .and_then(|c| c.as_array())
            .map(|arr| arr.iter().filter_map(|c| parse_plan_step(c, depth)).collect())
            .unwrap_or_default()
    };
    match control {
        ControlKind::Loop => PlanStep::Loop {
            times: payload.get("times").and_then(|t| t.as_u64()).unwrap_or(1) as u32,
            body: body(),
        },
        ControlKind::If => PlanStep::If {
            variable: payload.get("variable").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            operator: payload.get("operator").and_then(|v| v.as_str()).unwrap_or("EQUALS").to_string(),
            value: payload.get("value").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            body: body(),
        },
        ControlKind::Once => PlanStep::Once { body: body() },
        ControlKind::Timer => {
            PlanStep::Timer { ms: payload.get("ms").and_then(|m| m.as_u64()).unwrap_or(0) }
        }
    }
}

/// 把控制器(类型 + 载荷)解析为 PlanStep。子步骤可为叶子或嵌套控制器;解析失败的子步骤被跳过。
pub fn parse_control(control: ControlKind, payload: &serde_json::Value) -> PlanStep {
    parse_control_inner(control, payload, 1)
}

/// 已持久化的场景步骤。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioStep {
    pub id: String,
    pub order: i32,
    pub kind: StepKind,
    pub ref_mode: RefMode,
    pub snapshot: Option<serde_json::Value>,
}

/// 创建场景的入站请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewApiScenario {
    pub project_id: String,
    pub name: String,
    /// 创建人 user_id(组装根传入;无则 None)。
    pub created_by: Option<String>,
}

impl NewApiScenario {
    pub fn new(project_id: &str, name: &str) -> Result<Self, ScenarioError> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(ScenarioError::EmptyProjectId);
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(ScenarioError::EmptyName);
        }
        Ok(Self { project_id: project_id.to_string(), name: name.to_string(), created_by: None })
    }

    /// 设置创建人(链式)。
    pub fn with_created_by(mut self, user_id: Option<&str>) -> Self {
        self.created_by = user_id.map(|s| s.to_string());
        self
    }
}

/// 场景变更历史一条记录(审计日志)。created_at 文本承载。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioChange {
    pub id: String,
    pub scenario_id: String,
    pub action: String,
    pub detail: Option<String>,
    pub user_id: Option<String>,
    pub created_at: String,
}

/// 场景对某资源(接口用例)的引用引用记录。用于「引用关系」反查:
/// 给定一组用例 id,返回引用了它们的场景(去重后)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioReference {
    pub id: String,
    pub project_id: String,
    pub name: String,
}

/// 场景聚合根。steps 按 order 升序。
/// 注:`meta` 为不透明 JSON(描述/标签/等级/模块/参数),故不派生 `Eq`(serde_json::Value 无 Eq)。
#[derive(Debug, Clone, PartialEq)]
pub struct ApiScenario {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub status: ScenarioStatus,
    /// 元信息(前端约定形状:{description, tags, priority, moduleId, params, csvParams})。
    pub meta: serde_json::Value,
    /// 审计:创建人 user_id / 创建时间 / 更新时间(文本承载,见 0046 迁移)。
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub steps: Vec<ScenarioStep>,
}

/// 新增步骤的入站值。构造时按规则校验:
/// REQUEST 需合法 InlineRequest;CASE 需非空 case_id;SCENARIO 需非空 scenario_id;
/// COPY 模式必须携带快照(REFERENCE 可不带)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewScenarioStep {
    pub order: i32,
    pub kind: StepKind,
    pub ref_mode: RefMode,
    pub snapshot: Option<serde_json::Value>,
}

impl NewScenarioStep {
    pub fn new(
        order: i32,
        kind: StepKind,
        ref_mode: RefMode,
        snapshot: Option<serde_json::Value>,
    ) -> Result<Self, ScenarioError> {
        match &kind {
            StepKind::Request(_) => {} // InlineRequest 已在自身构造时校验
            StepKind::Case { case_id } => {
                if case_id.trim().is_empty() {
                    return Err(ScenarioError::EmptyCaseId);
                }
            }
            StepKind::Scenario { scenario_id } => {
                if scenario_id.trim().is_empty() {
                    return Err(ScenarioError::EmptyScenarioId);
                }
            }
            // 控制器载荷必须是 JSON 对象(具体字段在编译期宽容解析)。
            StepKind::Control { payload, .. } => {
                if !payload.is_object() {
                    return Err(ScenarioError::InvalidControl);
                }
            }
        }
        // COPY 模式必须有快照,REFERENCE 可为空。
        if ref_mode == RefMode::Copy && snapshot.is_none() {
            return Err(ScenarioError::MissingSnapshot);
        }
        Ok(Self { order, kind, ref_mode, snapshot })
    }
}

/// 编译产物:可运行步骤。case_id / request 恰有一个为 Some。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnableStep {
    pub case_id: Option<String>,
    pub request: Option<InlineRequest>,
}

impl RunnableStep {
    /// 由 case_id 构造一个可运行步骤。
    pub fn from_case(case_id: String) -> Self {
        Self { case_id: Some(case_id), request: None }
    }

    /// 由内联请求构造一个可运行步骤。
    pub fn from_request(request: InlineRequest) -> Self {
        Self { case_id: None, request: Some(request) }
    }
}

/// 纯领域:单层展开一个步骤。
/// REQUEST → RunnableStep{request};CASE → RunnableStep{case_id};
/// SCENARIO 不在此展开(需仓储查子场景,由应用层递归),返回 None。
pub fn flatten_step(step: &ScenarioStep) -> Option<RunnableStep> {
    match &step.kind {
        StepKind::Request(req) => Some(RunnableStep::from_request(req.clone())),
        StepKind::Case { case_id } => Some(RunnableStep::from_case(case_id.clone())),
        // SCENARIO 需仓储递归;CONTROL 需树形编译——都不在单层展开里。
        StepKind::Scenario { .. } | StepKind::Control { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_control_nests_controllers() {
        // LOOP ×2 内含 IF(make==yes)→ CASE c1
        let payload = serde_json::json!({
            "times": 2,
            "children": [
                { "kind": "IF", "variable": "make", "operator": "EQUALS", "value": "yes",
                  "children": [ { "kind": "CASE", "refId": "c1" } ] }
            ]
        });
        let plan = parse_control(ControlKind::Loop, &payload);
        match plan {
            PlanStep::Loop { times, body } => {
                assert_eq!(times, 2);
                assert_eq!(body.len(), 1);
                match &body[0] {
                    PlanStep::If { variable, operator, value, body } => {
                        assert_eq!(variable, "make");
                        assert_eq!(operator, "EQUALS");
                        assert_eq!(value, "yes");
                        assert_eq!(body, &vec![PlanStep::Case("c1".into())]);
                    }
                    other => panic!("expected nested If, got {other:?}"),
                }
            }
            other => panic!("expected Loop, got {other:?}"),
        }
    }

    #[test]
    fn parse_control_caps_nesting_depth() {
        // 构造超深嵌套 LOOP 链,应在上限处停止(不 panic/不爆栈)
        let mut node = serde_json::json!({ "kind": "CASE", "refId": "leaf" });
        for _ in 0..50 {
            node = serde_json::json!({ "kind": "LOOP", "times": 1, "children": [node] });
        }
        // 顶层再包一层解析;只要返回且不 panic 即可
        let plan = parse_control(ControlKind::Loop, &serde_json::json!({"times":1,"children":[node]}));
        assert!(matches!(plan, PlanStep::Loop { .. }));
    }

    #[test]
    fn status_as_str_and_parse_roundtrip() {
        for s in [
            ScenarioStatus::Draft,
            ScenarioStatus::Debugging,
            ScenarioStatus::Completed,
            ScenarioStatus::Deprecated,
        ] {
            assert_eq!(ScenarioStatus::parse(s.as_str()), s);
        }
        assert_eq!(ScenarioStatus::default(), ScenarioStatus::Draft);
        assert_eq!(ScenarioStatus::parse("???"), ScenarioStatus::Draft); // 未知回落
    }

    #[test]
    fn execution_status_as_str_and_parse_roundtrip() {
        for s in [
            ExecutionStatus::Pending,
            ExecutionStatus::Running,
            ExecutionStatus::Success,
            ExecutionStatus::Error,
        ] {
            assert_eq!(ExecutionStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(ExecutionStatus::default(), ExecutionStatus::Pending);
        assert_eq!(ExecutionStatus::parse("???"), None); // 未知 → None
    }

    #[test]
    fn ref_mode_as_str_and_parse_roundtrip() {
        assert_eq!(RefMode::Reference.as_str(), "REFERENCE");
        assert_eq!(RefMode::Copy.as_str(), "COPY");
        assert_eq!(RefMode::parse("COPY"), RefMode::Copy);
        assert_eq!(RefMode::parse("REFERENCE"), RefMode::Reference);
        assert_eq!(RefMode::parse("???"), RefMode::Reference); // 未知回落
        assert_eq!(RefMode::default(), RefMode::Reference);
    }

    #[test]
    fn inline_request_validates_method_and_url() {
        let r = InlineRequest::new("get", "http://x", None).expect("ok");
        assert_eq!(r.method, "GET"); // 大写归一
        assert_eq!(
            InlineRequest::new("FETCH", "http://x", None),
            Err(ScenarioError::InvalidMethod("FETCH".into()))
        );
        assert_eq!(
            InlineRequest::new("GET", "  ", None),
            Err(ScenarioError::EmptyUrl)
        );
    }

    #[test]
    fn kind_str_matches_variant() {
        assert_eq!(
            StepKind::Request(InlineRequest::new("GET", "u", None).expect("valid")).kind_str(),
            "REQUEST"
        );
        assert_eq!(StepKind::Case { case_id: "c".into() }.kind_str(), "CASE");
        assert_eq!(StepKind::Scenario { scenario_id: "s".into() }.kind_str(), "SCENARIO");
    }

    #[test]
    fn new_scenario_rejects_blanks() {
        assert_eq!(NewApiScenario::new("", "n"), Err(ScenarioError::EmptyProjectId));
        assert_eq!(NewApiScenario::new("p", "  "), Err(ScenarioError::EmptyName));
        assert!(NewApiScenario::new("p", "n").is_ok());
    }

    #[test]
    fn new_step_validates_case_and_scenario_ids() {
        assert_eq!(
            NewScenarioStep::new(0, StepKind::Case { case_id: " ".into() }, RefMode::Reference, None),
            Err(ScenarioError::EmptyCaseId)
        );
        assert_eq!(
            NewScenarioStep::new(
                0,
                StepKind::Scenario { scenario_id: "".into() },
                RefMode::Reference,
                None
            ),
            Err(ScenarioError::EmptyScenarioId)
        );
    }

    #[test]
    fn copy_mode_requires_snapshot() {
        let kind = StepKind::Case { case_id: "c".into() };
        assert_eq!(
            NewScenarioStep::new(0, kind.clone(), RefMode::Copy, None),
            Err(ScenarioError::MissingSnapshot)
        );
        // COPY + 快照 OK
        assert!(NewScenarioStep::new(
            0,
            kind.clone(),
            RefMode::Copy,
            Some(serde_json::json!({"a":1}))
        )
        .is_ok());
        // REFERENCE 可无快照
        assert!(NewScenarioStep::new(0, kind, RefMode::Reference, None).is_ok());
    }

    fn step(id: &str, order: i32, kind: StepKind) -> ScenarioStep {
        ScenarioStep { id: id.into(), order, kind, ref_mode: RefMode::Reference, snapshot: None }
    }

    #[test]
    fn flatten_request_yields_request_runnable() {
        let req = InlineRequest::new("POST", "http://x", Some("b".into())).expect("valid");
        let s = step("s1", 0, StepKind::Request(req.clone()));
        let r = flatten_step(&s).expect("some");
        assert_eq!(r, RunnableStep::from_request(req));
        assert!(r.case_id.is_none());
    }

    #[test]
    fn inline_request_carries_assertions() {
        let a = serde_json::json!([{"type": "StatusIs", "args": 200}]);
        let req = InlineRequest::new("GET", "http://x", None)
            .expect("valid")
            .with_assertions(a.clone());
        assert_eq!(req.assertions, a);
        // 默认空数组。
        assert_eq!(
            InlineRequest::new("GET", "http://x", None).expect("valid").assertions,
            serde_json::json!([])
        );
        // 非数组归一为空数组。
        let coerced = InlineRequest::new("GET", "http://x", None)
            .expect("valid")
            .with_assertions(serde_json::json!({"bad": 1}));
        assert_eq!(coerced.assertions, serde_json::json!([]));
    }

    #[test]
    fn control_child_request_parses_assertions() {
        // 控制器子步骤里的内联请求也应带断言(parse_plan_step REQUEST 分支)。
        let v = serde_json::json!({
            "kind": "REQUEST", "method": "GET", "url": "http://x",
            "assertions": [{"type": "StatusIs", "args": 201}]
        });
        let step = parse_plan_step(&v, 0).expect("parsed");
        match step {
            PlanStep::Request(r) => {
                assert_eq!(r.assertions, serde_json::json!([{"type": "StatusIs", "args": 201}]))
            }
            other => panic!("expected request, got {other:?}"),
        }
    }

    #[test]
    fn flatten_case_yields_case_runnable() {
        let s = step("s1", 0, StepKind::Case { case_id: "case-9".into() });
        let r = flatten_step(&s).expect("some");
        assert_eq!(r, RunnableStep::from_case("case-9".into()));
        assert!(r.request.is_none());
    }

    #[test]
    fn flatten_scenario_is_none_here() {
        let s = step("s1", 0, StepKind::Scenario { scenario_id: "scn-2".into() });
        assert!(flatten_step(&s).is_none()); // 子场景递归交给应用层
    }
}
