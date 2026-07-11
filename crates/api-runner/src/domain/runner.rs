use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Patch => "PATCH",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestSpec {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseSnapshot {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MatchCondition {
    Equals,
    NotEquals,
    Contains,
    NotContains,
    StartWith,
    EndWith,
    Empty,
    NotEmpty,
    Regex,
    Gt,
    GtOrEquals,
    Lt,
    LtOrEquals,
    LengthEquals,
    LengthNotEquals,
    LengthGt,
    LengthGtOrEquals,
    LengthLt,
    LengthLtOrEquals,
    Unchecked,
}

impl MatchCondition {
    pub fn matches(&self, actual: &str, expected: &str) -> bool {
        use MatchCondition::*;
        let num = |f: fn(f64, f64) -> bool| match (actual.parse::<f64>(), expected.parse::<f64>()) {
            (Ok(a), Ok(b)) => f(a, b),
            _ => false,
        };
        let len = |f: fn(usize, usize) -> bool| match expected.trim().parse::<usize>() {
            Ok(b) => f(actual.chars().count(), b),
            Err(_) => false,
        };
        match self {
            Unchecked => true,
            Equals => actual == expected,
            NotEquals => actual != expected,
            Contains => actual.contains(expected),
            NotContains => !actual.contains(expected),
            StartWith => actual.starts_with(expected),
            EndWith => actual.ends_with(expected),
            Empty => actual.is_empty(),
            NotEmpty => !actual.is_empty(),
            Regex => regex::Regex::new(expected).map(|re| re.is_match(actual)).unwrap_or(false),
            Gt => num(|a, b| a > b),
            GtOrEquals => num(|a, b| a >= b),
            Lt => num(|a, b| a < b),
            LtOrEquals => num(|a, b| a <= b),
            LengthEquals => len(|a, b| a == b),
            LengthNotEquals => len(|a, b| a != b),
            LengthGt => len(|a, b| a > b),
            LengthGtOrEquals => len(|a, b| a >= b),
            LengthLt => len(|a, b| a < b),
            LengthLtOrEquals => len(|a, b| a <= b),
        }
    }
}

fn json_path_to_pointer(path: &str) -> String {
    let p = path.trim();
    if p.starts_with('/') {
        return p.to_string();
    }
    let p = p.strip_prefix('$').unwrap_or(p);
    let normalized = p
        .replace("['", ".")
        .replace("']", "")
        .replace("[\"", ".")
        .replace("\"]", "")
        .replace('[', ".")
        .replace(']', "");
    let mut out = String::new();
    for seg in normalized.split('.') {
        if seg.is_empty() {
            continue;
        }
        out.push('/');
        // RFC6901 转义:~ → ~0 必须先于 / → ~1。
        out.push_str(&seg.replace('~', "~0").replace('/', "~1"));
    }
    out
}

fn json_value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "args")]
pub enum Assertion {
    StatusIs(u16),
    BodyContains(String),
    HeaderEquals { name: String, value: String },
    JsonFieldEquals { pointer: String, expected: String },
    ResponseCode { condition: MatchCondition, expected: String },
    ResponseHeader { name: String, condition: MatchCondition, expected: String },
    ResponseBody { condition: MatchCondition, expected: String },
    JsonPath { path: String, condition: MatchCondition, expected: String },
    ResponseTime { max_ms: u64 },
    Variable { name: String, condition: MatchCondition, expected: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseOutcome {
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseReport {
    pub outcome: CaseOutcome,
    pub failures: Vec<String>,
}

impl Assertion {
    fn needs_json(&self) -> bool {
        matches!(self, Assertion::JsonFieldEquals { .. } | Assertion::JsonPath { .. })
    }

    fn check(
        &self,
        resp: &ResponseSnapshot,
        json: Option<&serde_json::Value>,
        vars: &std::collections::BTreeMap<String, String>,
    ) -> Option<String> {
        match self {
            Assertion::StatusIs(want) => {
                (resp.status != *want).then(|| format!("status: 期望 {want},实际 {}", resp.status))
            }
            Assertion::BodyContains(needle) => {
                (!resp.body.contains(needle)).then(|| format!("body 不含子串: {needle}"))
            }
            Assertion::HeaderEquals { name, value } => {
                let got = resp
                    .headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v.as_str());
                (got != Some(value.as_str()))
                    .then(|| format!("header {name}: 期望 {value},实际 {got:?}"))
            }
            Assertion::JsonFieldEquals { pointer, expected } => match json {
                None => Some("body 不是合法 JSON".to_string()),
                Some(v) => {
                    let got = v.pointer(pointer);
                    let matches = match got {
                        Some(serde_json::Value::String(s)) => s == expected,
                        Some(other) => serde_json::from_str::<serde_json::Value>(expected)
                            .map(|e| &e == other)
                            .unwrap_or(false),
                        None => false,
                    };
                    (!matches).then(|| format!("json {pointer}: 期望 {expected},实际 {got:?}"))
                }
            },
            Assertion::ResponseCode { condition, expected } => {
                let actual = resp.status.to_string();
                (!condition.matches(&actual, expected))
                    .then(|| format!("status {condition:?}: 期望 {expected},实际 {actual}"))
            }
            Assertion::ResponseHeader { name, condition, expected } => {
                let got = resp
                    .headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                (!condition.matches(got, expected))
                    .then(|| format!("header {name} {condition:?}: 期望 {expected},实际 {got:?}"))
            }
            Assertion::ResponseBody { condition, expected } => (!condition
                .matches(&resp.body, expected))
            .then(|| format!("body {condition:?}: 期望 {expected}")),
            Assertion::JsonPath { path, condition, expected } => match json {
                None => Some("body 不是合法 JSON".to_string()),
                Some(v) => {
                    let pointer = json_path_to_pointer(path);
                    match v.pointer(&pointer) {
                        None => Some(format!("json {path}: 路径不存在")),
                        Some(found) => {
                            let actual = json_value_to_string(found);
                            (!condition.matches(&actual, expected)).then(|| {
                                format!("json {path} {condition:?}: 期望 {expected},实际 {actual}")
                            })
                        }
                    }
                }
            },
            Assertion::ResponseTime { max_ms } => (resp.elapsed_ms > *max_ms)
                .then(|| format!("耗时: 期望 ≤{max_ms}ms,实际 {}ms", resp.elapsed_ms)),
            Assertion::Variable { name, condition, expected } => {
                let actual = vars.get(name).map(String::as_str).unwrap_or("");
                (!condition.matches(actual, expected))
                    .then(|| format!("变量 {name} {condition:?}: 期望 {expected},实际 {actual:?}"))
            }
        }
    }
}

pub fn substitute(template: &str, vars: &std::collections::BTreeMap<String, String>) -> String {
    if !template.contains("${") {
        return template.to_string();
    }
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(val) => out.push_str(val),
                    None => {
                        out.push_str("${");
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str("${");
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn evaluate(assertions: &[Assertion], resp: &ResponseSnapshot) -> CaseReport {
    evaluate_with_vars(assertions, resp, &std::collections::BTreeMap::new())
}

pub fn evaluate_with_vars(
    assertions: &[Assertion],
    resp: &ResponseSnapshot,
    vars: &std::collections::BTreeMap<String, String>,
) -> CaseReport {
    let json = assertions
        .iter()
        .any(Assertion::needs_json)
        .then(|| serde_json::from_str::<serde_json::Value>(&resp.body).ok())
        .flatten();
    let failures: Vec<String> =
        assertions.iter().filter_map(|a| a.check(resp, json.as_ref(), vars)).collect();
    let outcome = if failures.is_empty() { CaseOutcome::Success } else { CaseOutcome::Error };
    CaseReport { outcome, failures }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionReport {
    pub item: String,
    pub condition: String,
    pub expected: String,
    pub actual: String,
    pub passed: bool,
    pub reason: String,
}

fn cond_label(c: &MatchCondition) -> String {
    use MatchCondition::*;
    match c {
        Equals => "等于",
        NotEquals => "不等于",
        Contains => "包含",
        NotContains => "不包含",
        StartWith => "开头为",
        EndWith => "结尾为",
        Empty => "为空",
        NotEmpty => "非空",
        Regex => "正则",
        Gt => "大于",
        GtOrEquals => "≥",
        Lt => "小于",
        LtOrEquals => "≤",
        LengthEquals => "长度=",
        LengthNotEquals => "长度≠",
        LengthGt => "长度>",
        LengthGtOrEquals => "长度≥",
        LengthLt => "长度<",
        LengthLtOrEquals => "长度≤",
        Unchecked => "不校验",
    }
    .to_string()
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{t}…")
    }
}

pub fn evaluate_detailed(
    assertions: &[Assertion],
    resp: &ResponseSnapshot,
) -> Vec<AssertionReport> {
    evaluate_detailed_with_vars(assertions, resp, &std::collections::BTreeMap::new())
}

pub fn evaluate_detailed_with_vars(
    assertions: &[Assertion],
    resp: &ResponseSnapshot,
    vars: &std::collections::BTreeMap<String, String>,
) -> Vec<AssertionReport> {
    let json = assertions
        .iter()
        .any(Assertion::needs_json)
        .then(|| serde_json::from_str::<serde_json::Value>(&resp.body).ok())
        .flatten();
    let header = |name: &str| {
        resp.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let json_at = |ptr: &str| {
        json.as_ref().and_then(|v| v.pointer(ptr)).map(|x| x.to_string()).unwrap_or_default()
    };
    assertions
        .iter()
        .map(|a| {
            let reason = a.check(resp, json.as_ref(), vars);
            let passed = reason.is_none();
            let (item, condition, expected, actual) = match a {
                Assertion::StatusIs(n) => (
                    "状态码".to_string(),
                    "等于".to_string(),
                    n.to_string(),
                    resp.status.to_string(),
                ),
                Assertion::BodyContains(s) => {
                    ("响应体".to_string(), "包含".to_string(), s.clone(), truncate(&resp.body, 60))
                }
                Assertion::HeaderEquals { name, value } => {
                    (format!("响应头[{name}]"), "等于".to_string(), value.clone(), header(name))
                }
                Assertion::JsonFieldEquals { pointer, expected } => (
                    format!("JSON {pointer}"),
                    "等于".to_string(),
                    expected.clone(),
                    json_at(pointer),
                ),
                Assertion::ResponseCode { condition, expected } => (
                    "状态码".to_string(),
                    cond_label(condition),
                    expected.clone(),
                    resp.status.to_string(),
                ),
                Assertion::ResponseHeader { name, condition, expected } => (
                    format!("响应头[{name}]"),
                    cond_label(condition),
                    expected.clone(),
                    header(name),
                ),
                Assertion::ResponseBody { condition, expected } => (
                    "响应体".to_string(),
                    cond_label(condition),
                    expected.clone(),
                    truncate(&resp.body, 60),
                ),
                Assertion::JsonPath { path, condition, expected } => (
                    format!("JSONPath {path}"),
                    cond_label(condition),
                    expected.clone(),
                    json_at(&json_path_to_pointer(path)),
                ),
                Assertion::ResponseTime { max_ms } => (
                    "响应耗时(ms)".to_string(),
                    "≤".to_string(),
                    max_ms.to_string(),
                    resp.elapsed_ms.to_string(),
                ),
                Assertion::Variable { name, condition, expected } => (
                    format!("变量[{name}]"),
                    cond_label(condition),
                    expected.clone(),
                    vars.get(name).cloned().unwrap_or_default(),
                ),
            };
            AssertionReport {
                item,
                condition,
                expected,
                actual,
                passed,
                reason: reason.unwrap_or_default(),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExtractKind {
    JsonPath,
    Regex,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExtractScope {
    #[default]
    Temp,
    Env,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extractor {
    pub variable: String,
    pub kind: ExtractKind,
    pub expression: String,
    #[serde(default)]
    pub scope: ExtractScope,
}

impl Extractor {
    pub fn extract(&self, resp: &ResponseSnapshot) -> Option<(String, String)> {
        let value = match self.kind {
            ExtractKind::JsonPath => {
                let v: serde_json::Value = serde_json::from_str(&resp.body).ok()?;
                let pointer = json_path_to_pointer(&self.expression);
                json_value_to_string(v.pointer(&pointer)?)
            }
            ExtractKind::Regex => {
                let re = regex::Regex::new(&self.expression).ok()?;
                let caps = re.captures(&resp.body)?;
                caps.get(1).or_else(|| caps.get(0))?.as_str().to_string()
            }
        };
        Some((self.variable.clone(), value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "args")]
pub enum Processor {
    Wait {
        ms: u64,
    },
    Extract {
        extractors: Vec<Extractor>,
    },
    /// 随用例往返存储;执行引擎尚未接入,执行器当前忽略。
    Script {
        lang: String,
        code: String,
    },
    /// 随用例往返存储;数据源执行尚未接入,执行器当前忽略。
    Sql {
        name: String,
        datasource: String,
        sql: String,
    },
}

pub fn wait_millis(processors: &[Processor]) -> u64 {
    processors
        .iter()
        .map(|p| match p {
            Processor::Wait { ms } => *ms,
            _ => 0,
        })
        .sum()
}

pub fn run_extracts(processors: &[Processor], resp: &ResponseSnapshot) -> Vec<(String, String)> {
    run_extracts_scoped(processors, resp, None)
}

pub fn env_extracts(processors: &[Processor], resp: &ResponseSnapshot) -> Vec<(String, String)> {
    run_extracts_scoped(processors, resp, Some(ExtractScope::Env))
}

fn run_extracts_scoped(
    processors: &[Processor],
    resp: &ResponseSnapshot,
    only: Option<ExtractScope>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for p in processors {
        if let Processor::Extract { extractors } = p {
            for e in extractors {
                if only.is_some_and(|s| s != e.scope) {
                    continue;
                }
                if let Some(kv) = e.extract(resp) {
                    out.push(kv);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(status: u16, body: &str, headers: &[(&str, &str)]) -> ResponseSnapshot {
        ResponseSnapshot {
            status,
            headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body: body.to_string(),
            elapsed_ms: 0,
        }
    }

    #[test]
    fn variable_assertion_reads_run_context_vars() {
        let r = resp(200, "", &[]);
        let mut vars = std::collections::BTreeMap::new();
        vars.insert("token".to_string(), "abc".to_string());
        let pass = Assertion::Variable {
            name: "token".into(),
            condition: MatchCondition::Equals,
            expected: "abc".into(),
        };
        let fail = Assertion::Variable {
            name: "token".into(),
            condition: MatchCondition::Equals,
            expected: "zzz".into(),
        };
        assert_eq!(evaluate_with_vars(&[pass], &r, &vars).outcome, CaseOutcome::Success);
        assert_eq!(evaluate_with_vars(&[fail], &r, &vars).outcome, CaseOutcome::Error);
        let miss = Assertion::Variable {
            name: "token".into(),
            condition: MatchCondition::Equals,
            expected: "abc".into(),
        };
        assert_eq!(evaluate(&[miss], &r).outcome, CaseOutcome::Error);
    }

    #[test]
    fn evaluate_detailed_reports_each_assertion() {
        let r = resp(200, r#"{"name":"alice"}"#, &[]);
        let a = vec![
            Assertion::StatusIs(200),
            Assertion::StatusIs(500),
            Assertion::BodyContains("alice".into()),
        ];
        let reports = evaluate_detailed(&a, &r);
        assert_eq!(reports.len(), 3);
        assert_eq!(reports[0].item, "状态码");
        assert_eq!(reports[0].condition, "等于");
        assert_eq!(reports[0].expected, "200");
        assert_eq!(reports[0].actual, "200");
        assert!(reports[0].passed);
        assert!(!reports[1].passed);
        assert!(reports[1].reason.contains("期望 500"));
        assert_eq!(reports[2].item, "响应体");
        assert!(reports[2].passed);
    }

    #[test]
    fn status_pass_and_fail() {
        let r = resp(200, "", &[]);
        assert_eq!(evaluate(&[Assertion::StatusIs(200)], &r).outcome, CaseOutcome::Success);
        let bad = evaluate(&[Assertion::StatusIs(201)], &r);
        assert_eq!(bad.outcome, CaseOutcome::Error);
        assert_eq!(bad.failures.len(), 1);
    }

    #[test]
    fn body_contains() {
        let r = resp(200, r#"{"name":"Alice"}"#, &[]);
        assert_eq!(
            evaluate(&[Assertion::BodyContains("Alice".into())], &r).outcome,
            CaseOutcome::Success
        );
        assert_eq!(
            evaluate(&[Assertion::BodyContains("Bob".into())], &r).outcome,
            CaseOutcome::Error
        );
    }

    #[test]
    fn header_equals_case_insensitive_name() {
        let r = resp(200, "", &[("Content-Type", "application/json")]);
        let a = Assertion::HeaderEquals {
            name: "content-type".into(),
            value: "application/json".into(),
        };
        assert_eq!(evaluate(&[a], &r).outcome, CaseOutcome::Success);
    }

    #[test]
    fn header_equals_missing_or_mismatch_fails() {
        let r = resp(200, "", &[("X-A", "1")]);
        let miss = Assertion::HeaderEquals { name: "X-B".into(), value: "1".into() };
        assert_eq!(evaluate(&[miss], &r).outcome, CaseOutcome::Error);
    }

    #[test]
    fn json_field_equals_string_and_number() {
        let r = resp(200, r#"{"data":{"id":"u1","num":42}}"#, &[]);
        let s = Assertion::JsonFieldEquals { pointer: "/data/id".into(), expected: "u1".into() };
        let n = Assertion::JsonFieldEquals { pointer: "/data/num".into(), expected: "42".into() };
        assert_eq!(evaluate(&[s, n], &r).outcome, CaseOutcome::Success);
    }

    #[test]
    fn json_field_missing_or_bad_json_fails() {
        let r = resp(200, "not json", &[]);
        let a = Assertion::JsonFieldEquals { pointer: "/x".into(), expected: "1".into() };
        assert_eq!(evaluate(&[a], &r).outcome, CaseOutcome::Error);
    }

    #[test]
    fn multiple_failures_are_all_collected() {
        let r = resp(500, "oops", &[]);
        let report = evaluate(
            &[
                Assertion::StatusIs(200),
                Assertion::BodyContains("ok".into()),
                Assertion::JsonFieldEquals { pointer: "/a".into(), expected: "1".into() },
            ],
            &r,
        );
        assert_eq!(report.outcome, CaseOutcome::Error);
        assert_eq!(report.failures.len(), 3);
    }

    #[test]
    fn assertion_json_format_is_stable() {
        // 锁定 JSONB 存储格式(PG 种子/读取依赖)
        let a = Assertion::StatusIs(200);
        assert_eq!(
            serde_json::to_value(&a).expect("ser"),
            serde_json::json!({"type":"StatusIs","args":200})
        );
        let j = Assertion::JsonFieldEquals { pointer: "/x".into(), expected: "1".into() };
        assert_eq!(
            serde_json::to_value(&j).expect("ser"),
            serde_json::json!({"type":"JsonFieldEquals","args":{"pointer":"/x","expected":"1"}})
        );
        let arr: Vec<Assertion> =
            serde_json::from_value(serde_json::json!([{"type":"StatusIs","args":201}]))
                .expect("de");
        assert_eq!(arr, vec![Assertion::StatusIs(201)]);
    }

    #[test]
    fn match_condition_string_ops() {
        use MatchCondition::*;
        assert!(Equals.matches("ok", "ok"));
        assert!(NotEquals.matches("ok", "no"));
        assert!(Contains.matches("hello world", "world"));
        assert!(NotContains.matches("hello", "x"));
        assert!(StartWith.matches("hello", "he"));
        assert!(EndWith.matches("hello", "lo"));
        assert!(Empty.matches("", "ignored"));
        assert!(NotEmpty.matches("a", "ignored"));
        assert!(Unchecked.matches("anything", "whatever"));
        assert!(Regex.matches("abc123", r"^[a-z]+\d+$"));
        assert!(!Regex.matches("ABC", r"^\d+$"));
        assert!(!Regex.matches("x", "("));
    }

    #[test]
    fn match_condition_numeric_and_length_ops() {
        use MatchCondition::*;
        assert!(Gt.matches("10", "5"));
        assert!(GtOrEquals.matches("5", "5"));
        assert!(Lt.matches("3", "9"));
        assert!(LtOrEquals.matches("9", "9"));
        assert!(!Gt.matches("abc", "5"));
        assert!(LengthEquals.matches("abc", "3"));
        assert!(LengthNotEquals.matches("abc", "4"));
        assert!(LengthGt.matches("abcd", "3"));
        assert!(LengthGtOrEquals.matches("abc", "3"));
        assert!(LengthLt.matches("ab", "3"));
        assert!(LengthLtOrEquals.matches("abc", "3"));
        assert!(!LengthEquals.matches("abc", "x"));
    }

    #[test]
    fn match_condition_serde_screaming_snake() {
        assert_eq!(
            serde_json::to_value(MatchCondition::GtOrEquals).expect("ser"),
            serde_json::json!("GT_OR_EQUALS")
        );
        assert_eq!(
            serde_json::to_value(MatchCondition::LengthLtOrEquals).expect("ser"),
            serde_json::json!("LENGTH_LT_OR_EQUALS")
        );
        let c: MatchCondition =
            serde_json::from_value(serde_json::json!("NOT_CONTAINS")).expect("de");
        assert_eq!(c, MatchCondition::NotContains);
    }

    #[test]
    fn response_code_header_body_assertions() {
        let r = resp(404, r#"{"msg":"not found"}"#, &[("X-Trace", "abc-123")]);
        assert_eq!(
            evaluate(
                &[Assertion::ResponseCode {
                    condition: MatchCondition::Equals,
                    expected: "404".into()
                }],
                &r
            )
            .outcome,
            CaseOutcome::Success
        );
        assert_eq!(
            evaluate(
                &[Assertion::ResponseHeader {
                    name: "x-trace".into(),
                    condition: MatchCondition::Regex,
                    expected: r"^abc-\d+$".into(),
                }],
                &r
            )
            .outcome,
            CaseOutcome::Success
        );
        assert_eq!(
            evaluate(
                &[Assertion::ResponseBody {
                    condition: MatchCondition::Contains,
                    expected: "not found".into()
                }],
                &r
            )
            .outcome,
            CaseOutcome::Success
        );
    }

    #[test]
    fn json_path_assertion_dotted_and_pointer() {
        let r = resp(200, r#"{"data":{"id":"u1","items":[10,20]}}"#, &[]);
        assert_eq!(
            evaluate(
                &[Assertion::JsonPath {
                    path: "$.data.id".into(),
                    condition: MatchCondition::Equals,
                    expected: "u1".into()
                }],
                &r
            )
            .outcome,
            CaseOutcome::Success
        );
        assert_eq!(
            evaluate(
                &[Assertion::JsonPath {
                    path: "$.data.items[1]".into(),
                    condition: MatchCondition::Equals,
                    expected: "20".into()
                }],
                &r
            )
            .outcome,
            CaseOutcome::Success
        );
        let miss = evaluate(
            &[Assertion::JsonPath {
                path: "$.data.ghost".into(),
                condition: MatchCondition::Equals,
                expected: "x".into(),
            }],
            &r,
        );
        assert_eq!(miss.outcome, CaseOutcome::Error);
    }

    #[test]
    fn json_path_to_pointer_forms() {
        assert_eq!(json_path_to_pointer("$.a.b"), "/a/b");
        assert_eq!(json_path_to_pointer("$.a[0].b"), "/a/0/b");
        assert_eq!(json_path_to_pointer("$['k'].v"), "/k/v");
        assert_eq!(json_path_to_pointer("/already/pointer"), "/already/pointer");
    }

    #[test]
    fn response_time_assertion() {
        let mut r = resp(200, "", &[]);
        r.elapsed_ms = 120;
        assert_eq!(
            evaluate(&[Assertion::ResponseTime { max_ms: 200 }], &r).outcome,
            CaseOutcome::Success
        );
        assert_eq!(
            evaluate(&[Assertion::ResponseTime { max_ms: 100 }], &r).outcome,
            CaseOutcome::Error
        );
    }

    #[test]
    fn legacy_assertions_still_deserialize() {
        let arr: Vec<Assertion> = serde_json::from_value(serde_json::json!([
            {"type":"StatusIs","args":200},
            {"type":"JsonFieldEquals","args":{"pointer":"/id","expected":"x"}}
        ]))
        .expect("de");
        assert_eq!(arr.len(), 2);
        let a =
            Assertion::ResponseBody { condition: MatchCondition::Contains, expected: "ok".into() };
        assert_eq!(
            serde_json::to_value(&a).expect("ser"),
            serde_json::json!({"type":"ResponseBody","args":{"condition":"CONTAINS","expected":"ok"}})
        );
    }

    #[test]
    fn extractor_jsonpath_and_regex() {
        let r = resp(200, r#"{"data":{"token":"tok-42","n":7}}"#, &[]);
        let jp = Extractor {
            variable: "tk".into(),
            kind: ExtractKind::JsonPath,
            expression: "$.data.token".into(),
            scope: ExtractScope::Temp,
        };
        assert_eq!(jp.extract(&r), Some(("tk".into(), "tok-42".into())));
        let jn = Extractor {
            variable: "num".into(),
            kind: ExtractKind::JsonPath,
            expression: "$.data.n".into(),
            scope: ExtractScope::Temp,
        };
        assert_eq!(jn.extract(&r), Some(("num".into(), "7".into())));
        let rx = Extractor {
            variable: "id".into(),
            kind: ExtractKind::Regex,
            expression: r#""token":"(tok-\d+)""#.into(),
            scope: ExtractScope::Temp,
        };
        assert_eq!(rx.extract(&r), Some(("id".into(), "tok-42".into())));
        let miss = Extractor {
            variable: "x".into(),
            kind: ExtractKind::JsonPath,
            expression: "$.nope".into(),
            scope: ExtractScope::Temp,
        };
        assert_eq!(miss.extract(&r), None);
    }

    #[test]
    fn processors_wait_and_extract_helpers() {
        let r = resp(200, r#"{"id":"u9"}"#, &[]);
        let procs = vec![
            Processor::Wait { ms: 100 },
            Processor::Wait { ms: 50 },
            Processor::Extract {
                extractors: vec![Extractor {
                    variable: "uid".into(),
                    kind: ExtractKind::JsonPath,
                    expression: "$.id".into(),
                    scope: ExtractScope::Temp,
                }],
            },
        ];
        assert_eq!(wait_millis(&procs), 150);
        assert_eq!(run_extracts(&procs, &r), vec![("uid".to_string(), "u9".to_string())]);
    }

    #[test]
    fn processor_serde_shape() {
        let p = Processor::Extract {
            extractors: vec![Extractor {
                variable: "v".into(),
                kind: ExtractKind::JsonPath,
                expression: "$.a".into(),
                scope: ExtractScope::Temp,
            }],
        };
        assert_eq!(
            serde_json::to_value(&p).expect("ser"),
            serde_json::json!({"type":"Extract","args":{"extractors":[
                {"variable":"v","kind":"JSON_PATH","expression":"$.a","scope":"TEMP"}
            ]}})
        );
        let w: Processor =
            serde_json::from_value(serde_json::json!({"type":"Wait","args":{"ms":250}}))
                .expect("de");
        assert_eq!(w, Processor::Wait { ms: 250 });
    }

    #[test]
    fn substitute_replaces_known_and_keeps_unknown() {
        let mut vars = std::collections::BTreeMap::new();
        vars.insert("host".to_string(), "example.com".to_string());
        vars.insert("tok".to_string(), "abc".to_string());
        assert_eq!(substitute("http://${host}/x", &vars), "http://example.com/x");
        assert_eq!(substitute("Bearer ${tok}", &vars), "Bearer abc");
        assert_eq!(substitute("${missing}/p", &vars), "${missing}/p");
        assert_eq!(substitute("plain", &vars), "plain");
        assert_eq!(substitute("a${host", &vars), "a${host");
        assert_eq!(substitute("${host}:${tok}", &vars), "example.com:abc");
    }

    #[test]
    fn all_pass_is_success() {
        let r = resp(201, r#"{"id":"x"}"#, &[("ETag", "abc")]);
        let report = evaluate(
            &[
                Assertion::StatusIs(201),
                Assertion::BodyContains("id".into()),
                Assertion::HeaderEquals { name: "etag".into(), value: "abc".into() },
                Assertion::JsonFieldEquals { pointer: "/id".into(), expected: "x".into() },
            ],
            &r,
        );
        assert_eq!(report.outcome, CaseOutcome::Success);
        assert!(report.failures.is_empty());
    }
}
