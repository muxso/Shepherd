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
}

/// 断言种类(对应 JMeter 的 Response Assertion / JSON Assertion)。
/// 邻接标签(tag+content)以支持基元 newtype 变体如 `StatusIs(u16)`:
/// `{"type":"StatusIs","args":200}` / `{"type":"JsonFieldEquals","args":{"pointer":..,"expected":..}}`。
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

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(status: u16, body: &str, headers: &[(&str, &str)]) -> ResponseSnapshot {
        ResponseSnapshot {
            status,
            headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body: body.to_string(),
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
