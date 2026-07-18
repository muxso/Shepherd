use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScenarioError {
    #[error("scenario name must not be empty")]
    EmptyName,
    #[error("scenario project_id must not be empty")]
    EmptyProjectId,
    #[error("invalid http method: {0}")]
    InvalidMethod(String),
    #[error("request url must not be empty")]
    EmptyUrl,
    #[error("case step requires a non-empty case_id")]
    EmptyCaseId,
    #[error("scenario step requires a non-empty scenario_id")]
    EmptyScenarioId,
    #[error("copy ref_mode requires a snapshot")]
    MissingSnapshot,
    #[error("control step payload must be a json object")]
    InvalidControl,
    #[error("cycle detected at scenario: {0}")]
    CycleDetected(String),
    #[error("max recursion depth exceeded: {0}")]
    MaxDepthExceeded(usize),
}

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

    pub fn parse(s: &str) -> ScenarioStatus {
        match s {
            "DEBUGGING" => ScenarioStatus::Debugging,
            "COMPLETED" => ScenarioStatus::Completed,
            "DEPRECATED" => ScenarioStatus::Deprecated,
            _ => ScenarioStatus::Draft,
        }
    }
}

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

    pub fn parse(s: &str) -> RefMode {
        match s {
            "COPY" => RefMode::Copy,
            _ => RefMode::Reference,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineRequest {
    pub method: String,
    pub url: String,
    pub body: Option<String>,
    pub assertions: serde_json::Value,
    pub headers: serde_json::Value,
    pub query_params: serde_json::Value,
    pub rest_params: serde_json::Value,
    pub auth: serde_json::Value,
    pub processors: serde_json::Value,
    /// Provenance for materialized copies (COPY_CASE / COPY_API / COPY_SCENARIO);
    /// None for hand-written requests.
    pub source: Option<String>,
}

const ALLOWED_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

fn empty_arr() -> serde_json::Value {
    serde_json::Value::Array(vec![])
}

impl InlineRequest {
    pub fn new(method: &str, url: &str, body: Option<String>) -> Result<Self, ScenarioError> {
        let method = method.trim().to_uppercase();
        if !ALLOWED_METHODS.contains(&method.as_str()) {
            return Err(ScenarioError::InvalidMethod(method));
        }
        let url = url.trim();
        if url.is_empty() {
            return Err(ScenarioError::EmptyUrl);
        }
        Ok(Self {
            method,
            url: url.to_string(),
            body,
            assertions: empty_arr(),
            headers: empty_arr(),
            query_params: empty_arr(),
            rest_params: empty_arr(),
            auth: serde_json::json!({}),
            processors: empty_arr(),
            source: None,
        })
    }

    pub fn with_assertions(mut self, assertions: serde_json::Value) -> Self {
        self.assertions = if assertions.is_array() { assertions } else { empty_arr() };
        self
    }

    pub fn with_spec(
        mut self,
        headers: serde_json::Value,
        query_params: serde_json::Value,
        rest_params: serde_json::Value,
        auth: serde_json::Value,
        processors: serde_json::Value,
    ) -> Self {
        let arr = |v: serde_json::Value| if v.is_array() { v } else { empty_arr() };
        self.headers = arr(headers);
        self.query_params = arr(query_params);
        self.rest_params = arr(rest_params);
        self.auth = if auth.is_object() { auth } else { serde_json::json!({}) };
        self.processors = arr(processors);
        self
    }

    pub fn with_spec_json(self, v: &serde_json::Value) -> Self {
        let mut out = self.with_spec(
            v.get("headers").cloned().unwrap_or_else(empty_arr),
            v.get("queryParams").cloned().unwrap_or_else(empty_arr),
            v.get("restParams").cloned().unwrap_or_else(empty_arr),
            v.get("auth").cloned().unwrap_or_else(|| serde_json::json!({})),
            v.get("processors").cloned().unwrap_or_else(empty_arr),
        );
        out.source = v
            .get("source")
            .and_then(|x| x.as_str())
            .filter(|x| !x.trim().is_empty())
            .map(str::to_string);
        out
    }

    pub fn to_inline_json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({ "method": self.method, "url": self.url });
        if let Some(b) = &self.body {
            v["body"] = serde_json::Value::String(b.clone());
        }
        let nonempty_arr = |x: &serde_json::Value| x.as_array().is_some_and(|a| !a.is_empty());
        if nonempty_arr(&self.assertions) {
            v["assertions"] = self.assertions.clone();
        }
        if nonempty_arr(&self.headers) {
            v["headers"] = self.headers.clone();
        }
        if nonempty_arr(&self.query_params) {
            v["queryParams"] = self.query_params.clone();
        }
        if nonempty_arr(&self.rest_params) {
            v["restParams"] = self.rest_params.clone();
        }
        if self.auth.as_object().is_some_and(|o| !o.is_empty()) {
            v["auth"] = self.auth.clone();
        }
        if nonempty_arr(&self.processors) {
            v["processors"] = self.processors.clone();
        }
        if let Some(src) = &self.source {
            v["source"] = serde_json::Value::String(src.clone());
        }
        v
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Loop,
    If,
    Once,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepKind {
    Request(Box<InlineRequest>),
    Case { case_id: String },
    Scenario { scenario_id: String },
    Control { control: ControlKind, payload: serde_json::Value },
}

impl StepKind {
    pub fn kind_str(&self) -> &'static str {
        match self {
            StepKind::Request(_) => "REQUEST",
            StepKind::Case { .. } => "CASE",
            StepKind::Scenario { .. } => "SCENARIO",
            StepKind::Control { control, .. } => control.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanStep {
    Case(String),
    Request(InlineRequest),
    Loop { times: u32, body: Vec<PlanStep> },
    If { variable: String, operator: String, value: String, body: Vec<PlanStep> },
    Once { body: Vec<PlanStep> },
    Timer { ms: u64 },
}

/// Nesting depth cap; guards against pathologically deep payloads exhausting the stack.
const MAX_CONTROL_DEPTH: usize = 10;

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
                .map(|r| r.with_assertions(assertions).with_spec_json(v))
                .map(PlanStep::Request)
        }
        other => match ControlKind::parse(other) {
            // A child step that is itself a controller: the same object carries both kind and payload — recurse.
            Some(ck) if depth < MAX_CONTROL_DEPTH => Some(parse_control_inner(ck, v, depth + 1)),
            _ => None,
        },
    }
}

fn parse_control_inner(
    control: ControlKind,
    payload: &serde_json::Value,
    depth: usize,
) -> PlanStep {
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
            variable: payload
                .get("variable")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            operator: payload
                .get("operator")
                .and_then(|v| v.as_str())
                .unwrap_or("EQUALS")
                .to_string(),
            value: payload.get("value").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            body: body(),
        },
        ControlKind::Once => PlanStep::Once { body: body() },
        ControlKind::Timer => {
            PlanStep::Timer { ms: payload.get("ms").and_then(|m| m.as_u64()).unwrap_or(0) }
        }
    }
}

pub fn parse_control(control: ControlKind, payload: &serde_json::Value) -> PlanStep {
    parse_control_inner(control, payload, 1)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioStep {
    pub id: String,
    pub order: i32,
    pub kind: StepKind,
    pub ref_mode: RefMode,
    pub snapshot: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewApiScenario {
    pub project_id: String,
    pub name: String,
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

    pub fn with_created_by(mut self, user_id: Option<&str>) -> Self {
        self.created_by = user_id.map(|s| s.to_string());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioChange {
    pub id: String,
    pub scenario_id: String,
    pub action: String,
    pub detail: Option<String>,
    pub user_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioReference {
    pub id: String,
    pub project_id: String,
    pub name: String,
}

/// No `Eq` derive: `meta` is opaque JSON and `serde_json::Value` lacks `Eq`.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiScenario {
    pub id: String,
    pub project_id: String,
    /// Per-project display number shown as the list ID (100001…).
    pub num: i64,
    pub name: String,
    pub status: ScenarioStatus,
    pub meta: serde_json::Value,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub steps: Vec<ScenarioStep>,
    pub last_result: Option<String>,
}

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
            StepKind::Request(_) => {}
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
            StepKind::Control { payload, .. } => {
                if !payload.is_object() {
                    return Err(ScenarioError::InvalidControl);
                }
            }
        }
        if ref_mode == RefMode::Copy && snapshot.is_none() {
            return Err(ScenarioError::MissingSnapshot);
        }
        Ok(Self { order, kind, ref_mode, snapshot })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnableStep {
    pub case_id: Option<String>,
    pub request: Option<InlineRequest>,
}

impl RunnableStep {
    pub fn from_case(case_id: String) -> Self {
        Self { case_id: Some(case_id), request: None }
    }

    pub fn from_request(request: InlineRequest) -> Self {
        Self { case_id: None, request: Some(request) }
    }
}

pub fn flatten_step(step: &ScenarioStep) -> Option<RunnableStep> {
    match &step.kind {
        StepKind::Request(req) => Some(RunnableStep::from_request((**req).clone())),
        StepKind::Case { case_id } => Some(RunnableStep::from_case(case_id.clone())),
        StepKind::Scenario { .. } | StepKind::Control { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_control_nests_controllers() {
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
        let mut node = serde_json::json!({ "kind": "CASE", "refId": "leaf" });
        for _ in 0..50 {
            node = serde_json::json!({ "kind": "LOOP", "times": 1, "children": [node] });
        }
        let plan =
            parse_control(ControlKind::Loop, &serde_json::json!({"times":1,"children":[node]}));
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
        assert_eq!(ScenarioStatus::parse("???"), ScenarioStatus::Draft);
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
        assert_eq!(ExecutionStatus::parse("???"), None);
    }

    #[test]
    fn ref_mode_as_str_and_parse_roundtrip() {
        assert_eq!(RefMode::Reference.as_str(), "REFERENCE");
        assert_eq!(RefMode::Copy.as_str(), "COPY");
        assert_eq!(RefMode::parse("COPY"), RefMode::Copy);
        assert_eq!(RefMode::parse("REFERENCE"), RefMode::Reference);
        assert_eq!(RefMode::parse("???"), RefMode::Reference);
        assert_eq!(RefMode::default(), RefMode::Reference);
    }

    #[test]
    fn inline_request_validates_method_and_url() {
        let r = InlineRequest::new("get", "http://x", None).expect("ok");
        assert_eq!(r.method, "GET");
        assert_eq!(
            InlineRequest::new("FETCH", "http://x", None),
            Err(ScenarioError::InvalidMethod("FETCH".into()))
        );
        assert_eq!(InlineRequest::new("GET", "  ", None), Err(ScenarioError::EmptyUrl));
    }

    #[test]
    fn kind_str_matches_variant() {
        assert_eq!(
            StepKind::Request(Box::new(InlineRequest::new("GET", "u", None).expect("valid")))
                .kind_str(),
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
            NewScenarioStep::new(
                0,
                StepKind::Case { case_id: " ".into() },
                RefMode::Reference,
                None
            ),
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
        assert!(NewScenarioStep::new(
            0,
            kind.clone(),
            RefMode::Copy,
            Some(serde_json::json!({"a":1}))
        )
        .is_ok());
        assert!(NewScenarioStep::new(0, kind, RefMode::Reference, None).is_ok());
    }

    fn step(id: &str, order: i32, kind: StepKind) -> ScenarioStep {
        ScenarioStep { id: id.into(), order, kind, ref_mode: RefMode::Reference, snapshot: None }
    }

    #[test]
    fn flatten_request_yields_request_runnable() {
        let req = InlineRequest::new("POST", "http://x", Some("b".into())).expect("valid");
        let s = step("s1", 0, StepKind::Request(Box::new(req.clone())));
        let r = flatten_step(&s).expect("some");
        assert_eq!(r, RunnableStep::from_request(req));
        assert!(r.case_id.is_none());
    }

    #[test]
    fn inline_request_carries_assertions() {
        let a = serde_json::json!([{"type": "StatusIs", "args": 200}]);
        let req =
            InlineRequest::new("GET", "http://x", None).expect("valid").with_assertions(a.clone());
        assert_eq!(req.assertions, a);
        assert_eq!(
            InlineRequest::new("GET", "http://x", None).expect("valid").assertions,
            serde_json::json!([])
        );
        let coerced = InlineRequest::new("GET", "http://x", None)
            .expect("valid")
            .with_assertions(serde_json::json!({"bad": 1}));
        assert_eq!(coerced.assertions, serde_json::json!([]));
    }

    #[test]
    fn control_child_request_parses_assertions() {
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
        assert!(flatten_step(&s).is_none());
    }
}
