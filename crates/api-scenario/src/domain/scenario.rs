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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineRequest {
    pub method: String,
    pub url: String,
    pub body: Option<String>,
}

const ALLOWED_METHODS: &[&str] =
    &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

impl InlineRequest {
    /// 校验构造:method 须在允许集合,url 非空。
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
        Ok(Self { method, url: url.to_string(), body })
    }
}

/// 步骤类型:内联请求 / 引用接口用例 / 引用子场景(可嵌套)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepKind {
    /// 内联请求。
    Request(InlineRequest),
    /// 引用接口用例。
    Case { case_id: String },
    /// 引用子场景(可嵌套)。
    Scenario { scenario_id: String },
}

impl StepKind {
    /// 步骤类型字符串(对应落库 kind 列)。
    pub fn kind_str(&self) -> &'static str {
        match self {
            StepKind::Request(_) => "REQUEST",
            StepKind::Case { .. } => "CASE",
            StepKind::Scenario { .. } => "SCENARIO",
        }
    }
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
        Ok(Self { project_id: project_id.to_string(), name: name.to_string() })
    }
}

/// 场景聚合根。steps 按 order 升序。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiScenario {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub status: ScenarioStatus,
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
        StepKind::Scenario { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
