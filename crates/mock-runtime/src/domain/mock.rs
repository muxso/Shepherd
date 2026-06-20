//! Mock 匹配引擎(纯函数,零 IO)。
//!
//! 一条 [`MockRule`] = 匹配条件([`MatchRule`])+ 命中后回放的响应([`MockResponse`])。
//! [`match_request`] 从规则集里选出命中的规则:逐项匹配,多条命中按 `priority` 取最高、并列取先到。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 进入 Mock 服务的请求视图(适配器从真实 HTTP 请求翻译而来)。
///
/// 约定:`headers` / `query` 的键统一小写,匹配时大小写不敏感由调用方在构造时保证(见 `normalized`)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MockRequest {
    pub method: String,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
}

impl MockRequest {
    /// 归一化:method 大写、header 键小写,便于大小写不敏感匹配。
    pub fn normalized(
        method: &str,
        path: &str,
        query: BTreeMap<String, String>,
        headers: BTreeMap<String, String>,
        body: Option<String>,
    ) -> Self {
        Self {
            method: method.trim().to_ascii_uppercase(),
            path: normalize_path(path),
            query,
            headers: headers.into_iter().map(|(k, v)| (k.to_ascii_lowercase(), v)).collect(),
            body,
        }
    }
}

/// 字符串字段的匹配方式。`Regex` 非法时视为不命中(绝不 panic)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", content = "value", rename_all = "snake_case")]
pub enum StringMatch {
    Equals(String),
    Contains(String),
    Regex(String),
}

impl StringMatch {
    pub fn test(&self, actual: &str) -> bool {
        match self {
            Self::Equals(v) => actual == v,
            Self::Contains(v) => actual.contains(v.as_str()),
            Self::Regex(p) => regex::Regex::new(p).map(|re| re.is_match(actual)).unwrap_or(false),
        }
    }
}

/// 请求体匹配。`Contains` 子串包含;`JsonPointer` 按 RFC 6901 取值后比较(体非 JSON / 路径不存在 → 不命中)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BodyMatch {
    Contains { value: String },
    JsonPointer { pointer: String, value: StringMatch },
}

impl BodyMatch {
    pub fn test(&self, body: Option<&str>) -> bool {
        let Some(body) = body else { return false };
        match self {
            Self::Contains { value } => body.contains(value.as_str()),
            Self::JsonPointer { pointer, value } => {
                let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
                    return false;
                };
                match json.pointer(pointer) {
                    Some(found) => value.test(&json_scalar_to_string(found)),
                    None => false,
                }
            }
        }
    }
}

/// 一条 Mock 的匹配条件。各项为 `None`/空 = 不约束(该维度任意命中)。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchRule {
    /// HTTP 方法(大小写不敏感);`None` = 任意方法。
    #[serde(default)]
    pub method: Option<String>,
    /// 路径模式:精确,或用 `*` 匹配单个路径段、末尾 `**` 匹配剩余所有段。
    pub path: String,
    /// 必须匹配的请求头(键大小写不敏感)。
    #[serde(default)]
    pub headers: Vec<(String, StringMatch)>,
    /// 必须匹配的查询参数。
    #[serde(default)]
    pub query: Vec<(String, StringMatch)>,
    /// 请求体匹配(全部满足)。
    #[serde(default)]
    pub body: Vec<BodyMatch>,
    /// 多条命中时优先级,高者胜;并列取先到。
    #[serde(default)]
    pub priority: i32,
}

/// 额外匹配条件:从 mock 的 `match_rule` jsonb 解析(均可选,缺省即不约束该维度)。
/// 用于在「定义的 method+path」之上叠加 header/query/body 细粒度条件与优先级。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtraConditions {
    pub headers: Vec<(String, StringMatch)>,
    pub query: Vec<(String, StringMatch)>,
    pub body: Vec<BodyMatch>,
    pub priority: i32,
}

impl MatchRule {
    /// 以接口定义的 method+path 为基,叠加 mock 的额外条件(match_rule jsonb)。
    pub fn from_definition(method: &str, path: &str, extra: ExtraConditions) -> Self {
        Self {
            method: Some(method.to_string()),
            path: path.to_string(),
            headers: extra.headers,
            query: extra.query,
            body: extra.body,
            priority: extra.priority,
        }
    }

    /// 该请求是否命中本规则(各维度皆满足)。
    pub fn matches(&self, req: &MockRequest) -> bool {
        // 方法
        if let Some(m) = &self.method {
            if !m.trim().eq_ignore_ascii_case(&req.method) {
                return false;
            }
        }
        // 路径
        if !path_matches(&self.path, &req.path) {
            return false;
        }
        // 请求头(键已小写)
        for (name, m) in &self.headers {
            match req.headers.get(&name.to_ascii_lowercase()) {
                Some(actual) if m.test(actual) => {}
                _ => return false,
            }
        }
        // 查询
        for (name, m) in &self.query {
            match req.query.get(name) {
                Some(actual) if m.test(actual) => {}
                _ => return false,
            }
        }
        // 请求体
        for bm in &self.body {
            if !bm.test(req.body.as_deref()) {
                return false;
            }
        }
        true
    }
}

/// 命中后回放的响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MockResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: Option<String>,
}

/// 一条完整 Mock 规则:条件 + 响应(`id` 便于审计/排错)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MockRule {
    pub id: String,
    #[serde(flatten)]
    pub rule: MatchRule,
    pub response: MockResponse,
}

/// 从规则集中选出命中请求的规则:`priority` 最高者胜,并列取**先到**(切片顺序在前)。
/// 无命中返回 `None`(调用方回落默认 404)。
pub fn match_request<'a>(req: &MockRequest, rules: &'a [MockRule]) -> Option<&'a MockRule> {
    let mut best: Option<&MockRule> = None;
    for r in rules.iter().filter(|r| r.rule.matches(req)) {
        match best {
            // 仅当严格更高优先级才替换 → 并列保留先到。
            Some(b) if b.rule.priority >= r.rule.priority => {}
            _ => best = Some(r),
        }
    }
    best
}

/// 路径归一化:去掉末尾斜杠(根 `/` 保留),空 → `/`。
fn normalize_path(path: &str) -> String {
    let p = path.trim();
    if p.is_empty() {
        return "/".to_string();
    }
    let trimmed = p.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 路径模式匹配:按 `/` 分段;`*` 匹配单段,末尾 `**` 匹配剩余所有段(含零段),其余精确。
fn path_matches(pattern: &str, path: &str) -> bool {
    let pat_norm = normalize_path(pattern);
    let path_norm = normalize_path(path);
    let pat: Vec<&str> = pat_norm.split('/').collect();
    let seg: Vec<&str> = path_norm.split('/').collect();
    let mut i = 0;
    while i < pat.len() {
        match pat[i] {
            "**" => return i + 1 == pat.len(), // `**` 只允许在末尾,吞掉剩余所有段
            _ if i >= seg.len() => return false,
            "*" => {} // 单段通配
            literal if literal == seg[i] => {}
            _ => return false,
        }
        i += 1;
    }
    // 模式用尽:仅当路径段也用尽才算精确匹配
    seg.len() == pat.len()
}

/// JSON 标量转字符串用于比较(字符串取原值,其余用紧凑 JSON 表示)。
fn json_scalar_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> MockRequest {
        MockRequest::normalized(
            "get",
            "/users/42/orders/",
            BTreeMap::from([("status".into(), "paid".into())]),
            BTreeMap::from([("Authorization".into(), "Bearer xyz".into())]),
            Some(r#"{"amount":100,"user":{"vip":true}}"#.into()),
        )
    }

    fn rule(id: &str, m: MatchRule, status: u16) -> MockRule {
        MockRule { id: id.into(), rule: m, response: MockResponse { status, headers: vec![], body: None } }
    }

    #[test]
    fn normalized_uppercases_method_and_lowercases_headers() {
        let r = req();
        assert_eq!(r.method, "GET");
        assert!(r.headers.contains_key("authorization"));
        assert_eq!(r.path, "/users/42/orders"); // 末尾斜杠去掉
    }

    #[test]
    fn path_glob_single_and_rest() {
        assert!(path_matches("/users/*/orders", "/users/42/orders"));
        assert!(!path_matches("/users/*/orders", "/users/42/orders/9")); // * 只一段
        assert!(path_matches("/files/**", "/files/a/b/c"));
        assert!(path_matches("/files/**", "/files")); // ** 含零段
        assert!(path_matches("/x", "/x/"));
        assert!(!path_matches("/x", "/y"));
        assert!(!path_matches("/a/b", "/a")); // 模式更长
    }

    #[test]
    fn method_none_matches_any() {
        let m = MatchRule { path: "/users/*/orders".into(), ..Default::default() };
        assert!(m.matches(&req()));
    }

    #[test]
    fn method_mismatch_fails() {
        let m = MatchRule { method: Some("post".into()), path: "/users/**".into(), ..Default::default() };
        assert!(!m.matches(&req()));
    }

    #[test]
    fn header_query_body_must_all_match() {
        let m = MatchRule {
            method: Some("GET".into()),
            path: "/users/*/orders".into(),
            headers: vec![("authorization".into(), StringMatch::Contains("Bearer".into()))],
            query: vec![("status".into(), StringMatch::Equals("paid".into()))],
            body: vec![BodyMatch::JsonPointer {
                pointer: "/user/vip".into(),
                value: StringMatch::Equals("true".into()),
            }],
            priority: 0,
        };
        assert!(m.matches(&req()));
    }

    #[test]
    fn header_case_insensitive_name() {
        let m = MatchRule {
            path: "/users/**".into(),
            headers: vec![("AUTHORIZATION".into(), StringMatch::Contains("xyz".into()))],
            ..Default::default()
        };
        assert!(m.matches(&req()));
    }

    #[test]
    fn body_json_pointer_missing_or_non_json_fails() {
        let m = MatchRule {
            path: "/users/**".into(),
            body: vec![BodyMatch::JsonPointer {
                pointer: "/missing".into(),
                value: StringMatch::Equals("x".into()),
            }],
            ..Default::default()
        };
        assert!(!m.matches(&req()));
    }

    #[test]
    fn invalid_regex_is_no_match_not_panic() {
        assert!(!StringMatch::Regex("[".into()).test("anything"));
        assert!(StringMatch::Regex("^Bearer".into()).test("Bearer xyz"));
    }

    #[test]
    fn from_definition_layers_extra_conditions() {
        // match_rule jsonb:额外要求 header X-Env=prod + 优先级 5
        let extra: ExtraConditions = serde_json::from_str(
            r#"{"headers":[["x-env",{"op":"equals","value":"prod"}]],"priority":5}"#,
        )
        .expect("parse extra");
        let rule = MatchRule::from_definition("GET", "/users/*/orders", extra);
        assert_eq!(rule.priority, 5);
        // 基础 method+path 命中,但缺 X-Env 头 → 不命中
        assert!(!rule.matches(&req()));
        // 带上 X-Env=prod → 命中
        let mut with_env = req();
        with_env.headers.insert("x-env".into(), "prod".into());
        assert!(rule.matches(&with_env));
    }

    #[test]
    fn empty_extra_is_just_method_path() {
        let rule = MatchRule::from_definition("GET", "/users/*/orders", ExtraConditions::default());
        assert!(rule.matches(&req()));
        assert_eq!(rule.priority, 0);
    }

    #[test]
    fn match_request_picks_highest_priority() {
        let rules = vec![
            rule("low", MatchRule { path: "/users/**".into(), priority: 1, ..Default::default() }, 200),
            rule("high", MatchRule { path: "/users/*/orders".into(), priority: 10, ..Default::default() }, 201),
        ];
        assert_eq!(match_request(&req(), &rules).expect("hit").id, "high");
    }

    #[test]
    fn match_request_tie_keeps_first() {
        let rules = vec![
            rule("first", MatchRule { path: "/users/**".into(), priority: 5, ..Default::default() }, 200),
            rule("second", MatchRule { path: "/users/*/orders".into(), priority: 5, ..Default::default() }, 201),
        ];
        assert_eq!(match_request(&req(), &rules).expect("hit").id, "first");
    }

    #[test]
    fn match_request_none_when_no_rule_hits() {
        let rules = vec![rule(
            "x",
            MatchRule { method: Some("DELETE".into()), path: "/users/**".into(), ..Default::default() },
            204,
        )];
        assert!(match_request(&req(), &rules).is_none());
    }

    #[test]
    fn rule_json_roundtrip() {
        // 规则可从 JSON(jsonb 存储/配置)反序列化,字段名 camelCase。
        let raw = r#"{
            "id": "r1",
            "method": "POST",
            "path": "/pay/*",
            "headers": [["x-token", {"op":"equals","value":"t"}]],
            "body": [{"kind":"contains","value":"amount"}],
            "priority": 3,
            "response": {"status": 201, "body": "{\"ok\":true}"}
        }"#;
        let r: MockRule = serde_json::from_str(raw).expect("parse");
        assert_eq!(r.rule.method.as_deref(), Some("POST"));
        assert_eq!(r.rule.priority, 3);
        assert_eq!(r.response.status, 201);
    }
}
