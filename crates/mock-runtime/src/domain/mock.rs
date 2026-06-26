use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MockRequest {
    pub method: String,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
}

impl MockRequest {
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

/// `Regex` 非法时视为不命中(绝不 panic)。
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

/// `JsonPointer`:体非 JSON / 路径不存在 → 不命中。
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

/// 各项为 `None`/空 = 不约束(该维度任意命中)。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchRule {
    #[serde(default)]
    pub method: Option<String>,
    /// 路径模式:精确,或用 `*` 匹配单个路径段、末尾 `**` 匹配剩余所有段。
    pub path: String,
    #[serde(default)]
    pub headers: Vec<(String, StringMatch)>,
    #[serde(default)]
    pub query: Vec<(String, StringMatch)>,
    #[serde(default)]
    pub body: Vec<BodyMatch>,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtraConditions {
    pub headers: Vec<(String, StringMatch)>,
    pub query: Vec<(String, StringMatch)>,
    pub body: Vec<BodyMatch>,
    pub priority: i32,
}

impl MatchRule {
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

    pub fn matches(&self, req: &MockRequest) -> bool {
        if let Some(m) = &self.method {
            if !m.trim().eq_ignore_ascii_case(&req.method) {
                return false;
            }
        }
        if !path_matches(&self.path, &req.path) {
            return false;
        }
        for (name, m) in &self.headers {
            match req.headers.get(&name.to_ascii_lowercase()) {
                Some(actual) if m.test(actual) => {}
                _ => return false,
            }
        }
        for (name, m) in &self.query {
            match req.query.get(name) {
                Some(actual) if m.test(actual) => {}
                _ => return false,
            }
        }
        for bm in &self.body {
            if !bm.test(req.body.as_deref()) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MockResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MockRule {
    pub id: String,
    #[serde(flatten)]
    pub rule: MatchRule,
    pub response: MockResponse,
}

pub fn match_request<'a>(req: &MockRequest, rules: &'a [MockRule]) -> Option<&'a MockRule> {
    let mut best: Option<&MockRule> = None;
    for r in rules.iter().filter(|r| r.rule.matches(req)) {
        match best {
            // `>=` 而非 `>`:仅严格更高才替换,并列保留先到。
            Some(b) if b.rule.priority >= r.rule.priority => {}
            _ => best = Some(r),
        }
    }
    best
}

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

fn path_matches(pattern: &str, path: &str) -> bool {
    let pat_norm = normalize_path(pattern);
    let path_norm = normalize_path(path);
    let pat: Vec<&str> = pat_norm.split('/').collect();
    let seg: Vec<&str> = path_norm.split('/').collect();
    let mut i = 0;
    while i < pat.len() {
        match pat[i] {
            "**" => return i + 1 == pat.len(), // `**` 只允许在末尾
            _ if i >= seg.len() => return false,
            "*" => {}
            literal if literal == seg[i] => {}
            _ => return false,
        }
        i += 1;
    }
    seg.len() == pat.len()
}

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
        MockRule { id: id.into(), rule: m, response: MockResponse { status, headers: vec![], body: None, delay_ms: 0 } }
    }

    #[test]
    fn normalized_uppercases_method_and_lowercases_headers() {
        let r = req();
        assert_eq!(r.method, "GET");
        assert!(r.headers.contains_key("authorization"));
        assert_eq!(r.path, "/users/42/orders");
    }

    #[test]
    fn path_glob_single_and_rest() {
        assert!(path_matches("/users/*/orders", "/users/42/orders"));
        assert!(!path_matches("/users/*/orders", "/users/42/orders/9"));
        assert!(path_matches("/files/**", "/files/a/b/c"));
        assert!(path_matches("/files/**", "/files"));
        assert!(path_matches("/x", "/x/"));
        assert!(!path_matches("/x", "/y"));
        assert!(!path_matches("/a/b", "/a"));
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
        let extra: ExtraConditions = serde_json::from_str(
            r#"{"headers":[["x-env",{"op":"equals","value":"prod"}]],"priority":5}"#,
        )
        .expect("parse extra");
        let rule = MatchRule::from_definition("GET", "/users/*/orders", extra);
        assert_eq!(rule.priority, 5);
        assert!(!rule.matches(&req()));
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
