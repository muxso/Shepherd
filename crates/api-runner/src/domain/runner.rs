//! 接口用例模型 + 断言判定(纯函数,零 IO,可穷举测试)。
//!
//! 请求规格与断言派生 serde,便于从 PG(JSONB)读取用例定义。

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

/// 一个接口用例的请求规格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestSpec {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

/// 执行后的响应快照(执行器产出,喂给纯函数判定)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseSnapshot {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// 请求耗时(毫秒),供 RESPONSE_TIME 断言。执行器测量;构造测试快照时给 0 即可。
    pub elapsed_ms: u64,
}

/// 断言匹配操作符(对齐前端 `RequestAssertionCondition`)。SCREAMING_SNAKE 序列化:
/// `EQUALS` / `NOT_EQUALS` / `CONTAINS` / `GT_OR_EQUALS` / `LENGTH_GT` / `REGEX` / `UNCHECKED` …
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
    /// 数值比较(两侧能 parse 为 f64 才成立)。
    Gt,
    GtOrEquals,
    Lt,
    LtOrEquals,
    /// 长度比较(actual 的字符数 与 expected 解析出的整数)。
    LengthEquals,
    LengthNotEquals,
    LengthGt,
    LengthGtOrEquals,
    LengthLt,
    LengthLtOrEquals,
    /// 不校验:恒通过。
    Unchecked,
}

impl MatchCondition {
    /// 用该操作符比较 `actual` 与 `expected`,满足返回 true。
    pub fn matches(&self, actual: &str, expected: &str) -> bool {
        use MatchCondition::*;
        // 数值比较:两侧都能 parse 为 f64。
        let num = |f: fn(f64, f64) -> bool| match (actual.parse::<f64>(), expected.parse::<f64>()) {
            (Ok(a), Ok(b)) => f(a, b),
            _ => false,
        };
        // 长度比较:actual 字符数 vs expected 解析出的 usize。
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

/// 把前端常见的 JSONPath(`$.a.b` / `$.a[0]` / `$['k']`)规整为 JSON Pointer(`/a/b` / `/a/0` / `/k`)。
/// 已是 Pointer(以 `/` 开头)则原样返回。仅支持点/下标的简单路径,不支持过滤/通配。
fn json_path_to_pointer(path: &str) -> String {
    let p = path.trim();
    if p.starts_with('/') {
        return p.to_string();
    }
    let p = p.strip_prefix('$').unwrap_or(p);
    // 把 ['k'] / ["k"] / [n] 统一成 .k / .n
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
        // RFC6901 转义(~ → ~0,/ → ~1)。
        out.push_str(&seg.replace('~', "~0").replace('/', "~1"));
    }
    out
}

/// 把 JSON 值取成可比较的字符串(字符串原样,其余用紧凑 JSON 文本)。
fn json_value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// 断言种类(对应 JMeter 的 Response Assertion / JSON Assertion)。
/// 邻接标签(tag+content)以支持基元 newtype 变体如 `StatusIs(u16)`:
/// `{"type":"StatusIs","args":200}` / `{"type":"JsonFieldEquals","args":{"pointer":..,"expected":..}}`。
/// 带操作符的新变体(对齐前端 MeterSphere 断言矩阵):
/// `{"type":"ResponseBody","args":{"condition":"CONTAINS","expected":"ok"}}`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "args")]
pub enum Assertion {
    /// 状态码等于。
    StatusIs(u16),
    /// 响应体包含子串。
    BodyContains(String),
    /// 响应头(名不区分大小写)等于某值。
    HeaderEquals { name: String, value: String },
    /// 响应体按 JSON Pointer(RFC 6901,如 `/data/id`)取值后等于某字符串。
    JsonFieldEquals { pointer: String, expected: String },
    /// 状态码按操作符匹配(RESPONSE_CODE)。
    ResponseCode { condition: MatchCondition, expected: String },
    /// 响应头(名不区分大小写)按操作符匹配(RESPONSE_HEADER)。
    ResponseHeader { name: String, condition: MatchCondition, expected: String },
    /// 整个响应体按操作符匹配(RESPONSE_BODY,含 REGEX/CONTAINS 等)。
    ResponseBody { condition: MatchCondition, expected: String },
    /// 响应体按 JSONPath 取值后按操作符匹配(JSON_PATH;path 支持 `$.a.b` 或 Pointer)。
    JsonPath { path: String, condition: MatchCondition, expected: String },
    /// 响应耗时不超过 max_ms 毫秒(RESPONSE_TIME)。
    ResponseTime { max_ms: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseOutcome {
    Success,
    Error,
}

/// 一个用例的判定结果:成功,或带具体失败原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseReport {
    pub outcome: CaseOutcome,
    pub failures: Vec<String>,
}

impl Assertion {
    /// 对单条断言求值,通过返回 None,失败返回 Some(原因)。
    fn check(&self, resp: &ResponseSnapshot) -> Option<String> {
        match self {
            Assertion::StatusIs(want) => (resp.status != *want)
                .then(|| format!("status: 期望 {want},实际 {}", resp.status)),
            Assertion::BodyContains(needle) => (!resp.body.contains(needle))
                .then(|| format!("body 不含子串: {needle}")),
            Assertion::HeaderEquals { name, value } => {
                let got = resp
                    .headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v.as_str());
                (got != Some(value.as_str()))
                    .then(|| format!("header {name}: 期望 {value},实际 {got:?}"))
            }
            Assertion::JsonFieldEquals { pointer, expected } => {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(&resp.body);
                match parsed {
                    Err(_) => Some("body 不是合法 JSON".to_string()),
                    Ok(v) => {
                        let got = v.pointer(pointer);
                        let matches = match got {
                            // 字符串值:直接比字符串
                            Some(serde_json::Value::String(s)) => s == expected,
                            // 数字/布尔等:把期望也解析为 JSON 做类型化比较(42 == 42,而非 "42")
                            Some(other) => serde_json::from_str::<serde_json::Value>(expected)
                                .map(|e| &e == other)
                                .unwrap_or(false),
                            None => false,
                        };
                        (!matches).then(|| {
                            format!("json {pointer}: 期望 {expected},实际 {got:?}")
                        })
                    }
                }
            }
            Assertion::ResponseCode { condition, expected } => {
                let actual = resp.status.to_string();
                (!condition.matches(&actual, expected)).then(|| {
                    format!("status {condition:?}: 期望 {expected},实际 {actual}")
                })
            }
            Assertion::ResponseHeader { name, condition, expected } => {
                let got = resp
                    .headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                (!condition.matches(got, expected)).then(|| {
                    format!("header {name} {condition:?}: 期望 {expected},实际 {got:?}")
                })
            }
            Assertion::ResponseBody { condition, expected } => {
                (!condition.matches(&resp.body, expected)).then(|| {
                    format!("body {condition:?}: 期望 {expected}")
                })
            }
            Assertion::JsonPath { path, condition, expected } => {
                match serde_json::from_str::<serde_json::Value>(&resp.body) {
                    Err(_) => Some("body 不是合法 JSON".to_string()),
                    Ok(v) => {
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
                }
            }
            Assertion::ResponseTime { max_ms } => (resp.elapsed_ms > *max_ms)
                .then(|| format!("耗时: 期望 ≤{max_ms}ms,实际 {}ms", resp.elapsed_ms)),
        }
    }
}

/// `${name}` 变量替换。**纯函数**:把 `template` 中形如 `${key}` 的占位符替换为
/// `vars[key]`;未知键或残缺花括号原样保留。供执行器对 url/headers/body 注入环境变量。
pub fn substitute(template: &str, vars: &std::collections::BTreeMap<String, String>) -> String {
    // 无占位符快路径,避免无谓分配。
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
                        // 未知键:原样保留 `${key}`。
                        out.push_str("${");
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                // 没有闭合花括号:剩余原样输出并结束。
                out.push_str("${");
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 对一组断言求值得到用例结果。**纯函数**:同样输入恒得同样结果。
pub fn evaluate(assertions: &[Assertion], resp: &ResponseSnapshot) -> CaseReport {
    let failures: Vec<String> = assertions.iter().filter_map(|a| a.check(resp)).collect();
    let outcome = if failures.is_empty() { CaseOutcome::Success } else { CaseOutcome::Error };
    CaseReport { outcome, failures }
}

// ---------- 前后置处理器(EXTRACT 参数提取 / TIME_WAITING 等待) ----------

/// 参数提取方式(对齐前端 `RequestExtractExpressionEnum`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExtractKind {
    JsonPath,
    Regex,
}

/// 单条提取规则:从响应取值写入变量 `variable`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extractor {
    pub variable: String,
    pub kind: ExtractKind,
    /// JSONPath(`$.a.b`/Pointer)或正则(取第 1 捕获组,无组取整体匹配)。
    pub expression: String,
}

impl Extractor {
    /// 从响应提取 `(变量名, 值)`;取不到返回 None。
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

/// 前/后置处理器(本阶段:等待 + 参数提取;脚本/SQL 留待后续)。
/// 邻接标签:`{"type":"Wait","args":{"ms":500}}` / `{"type":"Extract","args":{"extractors":[..]}}`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "args")]
pub enum Processor {
    /// 等待固定毫秒(TIME_WAITING / CONSTANT_TIMER)。
    Wait { ms: u64 },
    /// 参数提取(EXTRACT):把若干提取结果写入运行上下文变量。
    Extract { extractors: Vec<Extractor> },
}

/// 处理器集合里所有 Wait 的总毫秒数(执行器据此 sleep)。**纯函数**。
pub fn wait_millis(processors: &[Processor]) -> u64 {
    processors
        .iter()
        .map(|p| match p {
            Processor::Wait { ms } => *ms,
            _ => 0,
        })
        .sum()
}

/// 对响应跑所有 Extract 处理器,产出 `(变量名, 值)` 列表(按出现顺序)。**纯函数**。
pub fn run_extracts(processors: &[Processor], resp: &ResponseSnapshot) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for p in processors {
        if let Processor::Extract { extractors } = p {
            for e in extractors {
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
            name: "content-type".into(), // 名大小写无关
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
        assert_eq!(report.failures.len(), 3); // 三条都收集,便于报告
    }

    #[test]
    fn assertion_json_format_is_stable() {
        // 锁定 JSONB 存储格式(PG 种子/读取依赖)
        let a = Assertion::StatusIs(200);
        assert_eq!(serde_json::to_value(&a).expect("ser"), serde_json::json!({"type":"StatusIs","args":200}));
        let j = Assertion::JsonFieldEquals { pointer: "/x".into(), expected: "1".into() };
        assert_eq!(
            serde_json::to_value(&j).expect("ser"),
            serde_json::json!({"type":"JsonFieldEquals","args":{"pointer":"/x","expected":"1"}})
        );
        // 反序列化往返
        let arr: Vec<Assertion> =
            serde_json::from_value(serde_json::json!([{"type":"StatusIs","args":201}])).expect("de");
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
        // 非法正则 → 不匹配,不 panic
        assert!(!Regex.matches("x", "("));
    }

    #[test]
    fn match_condition_numeric_and_length_ops() {
        use MatchCondition::*;
        assert!(Gt.matches("10", "5"));
        assert!(GtOrEquals.matches("5", "5"));
        assert!(Lt.matches("3", "9"));
        assert!(LtOrEquals.matches("9", "9"));
        // 非数值 → false
        assert!(!Gt.matches("abc", "5"));
        // 长度:actual 字符数 vs expected 整数
        assert!(LengthEquals.matches("abc", "3"));
        assert!(LengthNotEquals.matches("abc", "4"));
        assert!(LengthGt.matches("abcd", "3"));
        assert!(LengthGtOrEquals.matches("abc", "3"));
        assert!(LengthLt.matches("ab", "3"));
        assert!(LengthLtOrEquals.matches("abc", "3"));
        // expected 非整数 → false
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
        // RESPONSE_CODE 操作符
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
        // RESPONSE_HEADER 正则
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
        // RESPONSE_BODY 包含
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
        // 点式 $.data.id
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
        // 下标 $.data.items[1]
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
        // 路径不存在 → 失败
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
        // 既有数据格式不破:StatusIs / JsonFieldEquals 照常工作
        let arr: Vec<Assertion> = serde_json::from_value(serde_json::json!([
            {"type":"StatusIs","args":200},
            {"type":"JsonFieldEquals","args":{"pointer":"/id","expected":"x"}}
        ]))
        .expect("de");
        assert_eq!(arr.len(), 2);
        // 新变体序列化形态
        let a = Assertion::ResponseBody {
            condition: MatchCondition::Contains,
            expected: "ok".into(),
        };
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
        };
        assert_eq!(jp.extract(&r), Some(("tk".into(), "tok-42".into())));
        // 数字取值转字符串
        let jn = Extractor {
            variable: "num".into(),
            kind: ExtractKind::JsonPath,
            expression: "$.data.n".into(),
        };
        assert_eq!(jn.extract(&r), Some(("num".into(), "7".into())));
        // 正则取第 1 捕获组
        let rx = Extractor {
            variable: "id".into(),
            kind: ExtractKind::Regex,
            expression: r#""token":"(tok-\d+)""#.into(),
        };
        assert_eq!(rx.extract(&r), Some(("id".into(), "tok-42".into())));
        // 取不到 → None
        let miss = Extractor {
            variable: "x".into(),
            kind: ExtractKind::JsonPath,
            expression: "$.nope".into(),
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
            }],
        };
        assert_eq!(
            serde_json::to_value(&p).expect("ser"),
            serde_json::json!({"type":"Extract","args":{"extractors":[
                {"variable":"v","kind":"JSON_PATH","expression":"$.a"}
            ]}})
        );
        let w: Processor =
            serde_json::from_value(serde_json::json!({"type":"Wait","args":{"ms":250}})).expect("de");
        assert_eq!(w, Processor::Wait { ms: 250 });
    }

    #[test]
    fn substitute_replaces_known_and_keeps_unknown() {
        let mut vars = std::collections::BTreeMap::new();
        vars.insert("host".to_string(), "example.com".to_string());
        vars.insert("tok".to_string(), "abc".to_string());
        assert_eq!(substitute("http://${host}/x", &vars), "http://example.com/x");
        assert_eq!(substitute("Bearer ${tok}", &vars), "Bearer abc");
        // 未知键原样保留
        assert_eq!(substitute("${missing}/p", &vars), "${missing}/p");
        // 无占位符快路径
        assert_eq!(substitute("plain", &vars), "plain");
        // 残缺花括号原样
        assert_eq!(substitute("a${host", &vars), "a${host");
        // 多个占位符
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
