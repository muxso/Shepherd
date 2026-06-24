//! HAR 1.2 抓包导入:把 `log.entries` 摊平成一批待建接口。
//!
//! 纯函数、零 IO。每条 entry 取其 `request`(method/url/headers/queryString/postData)摊平为
//! [`ImportedApi`],响应状态码用作默认用例断言。HAR 常含同一接口的多次调用:按「方法+路径」去重
//! (保留首次)。host 作为模块名,便于 group_by_tag 按域名归类。

use std::collections::HashSet;

use serde_json::Value;

use crate::domain::error::ApiDefinitionError;
use crate::domain::import::{
    body_type_of, kv, path_and_query, simple_spec, status_assertions, ImportedApi,
};

/// 解析 HAR 文档。无 `log.entries` 或其中无任何请求时报 `BadImport`。
pub fn parse_har(doc: &Value) -> Result<Vec<ImportedApi>, ApiDefinitionError> {
    let entries = doc
        .get("log")
        .and_then(|l| l.get("entries"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiDefinitionError::BadImport("har: 缺少 `log.entries` 数组".into()))?;

    let mut out = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for e in entries {
        let Some(api) = entry_to_api(e) else { continue };
        if seen.insert((api.method.clone(), api.path.clone())) {
            out.push(api);
        }
    }
    if out.is_empty() {
        return Err(ApiDefinitionError::BadImport("har: 未找到任何请求".into()));
    }
    Ok(out)
}

fn entry_to_api(entry: &Value) -> Option<ImportedApi> {
    let req = entry.get("request")?;
    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("GET").to_uppercase();
    let raw_url = req.get("url").and_then(|v| v.as_str())?;
    let (path, url_query) = path_and_query(raw_url);
    let host = host_of(raw_url);

    // 请求头:过滤伪头(:authority 等)。
    let mut headers = Vec::new();
    if let Some(hs) = req.get("headers").and_then(|v| v.as_array()) {
        for h in hs {
            let (Some(k), v) = (
                h.get("name").and_then(|v| v.as_str()),
                h.get("value").and_then(|v| v.as_str()).unwrap_or(""),
            ) else {
                continue;
            };
            if k.starts_with(':') {
                continue; // HTTP/2 伪头
            }
            headers.push(kv(k, v, ""));
        }
    }

    // 查询:优先结构化 queryString,回落 URL 解析结果。
    let query: Vec<Value> = match req.get("queryString").and_then(|v| v.as_array()) {
        Some(qs) if !qs.is_empty() => qs
            .iter()
            .filter_map(|q| {
                let name = q.get("name").and_then(|v| v.as_str())?;
                let value = q.get("value").and_then(|v| v.as_str()).unwrap_or("");
                Some(kv(name, value, ""))
            })
            .collect(),
        _ => url_query.iter().map(|(k, v)| kv(k, v, "")).collect(),
    };

    // 请求体:postData.text。
    let body_text = req
        .get("postData")
        .and_then(|p| p.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let body_type = body_type_of(&body_text).to_string();

    let status = entry
        .get("response")
        .and_then(|r| r.get("status"))
        .and_then(|v| v.as_u64())
        .map(|n| n as u16)
        .filter(|s| (100..600).contains(s))
        .unwrap_or(200);

    let case_body = if matches!(method.as_str(), "POST" | "PUT" | "PATCH") && !body_text.is_empty() {
        Some(body_text.clone())
    } else {
        None
    };

    Some(ImportedApi {
        name: format!("{method} {path}"),
        method,
        path,
        spec: simple_spec("", headers, query, Vec::new(), &body_type, &body_text, Vec::new()),
        case_assertions: status_assertions(status),
        case_body,
        module: host,
    })
}

/// 从完整 URL 取 host(用作模块名);无协议/host 时为 None。
fn host_of(url: &str) -> Option<String> {
    let after = url.split("://").nth(1)?;
    let host = after.split(['/', '?', '#']).next()?;
    let host = host.split('@').next_back()?; // 去掉可能的 user:pass@
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_entries_and_dedups() {
        let doc = json!({
            "log": { "entries": [
                { "request": {
                    "method": "GET",
                    "url": "https://api.example.com/users?page=1",
                    "headers": [
                        { "name": ":authority", "value": "api.example.com" },
                        { "name": "Accept", "value": "application/json" }
                    ],
                    "queryString": [{ "name": "page", "value": "1" }]
                }, "response": { "status": 200 } },
                { "request": { "method": "GET", "url": "https://api.example.com/users?page=2" }, "response": { "status": 200 } },
                { "request": {
                    "method": "POST",
                    "url": "https://api.example.com/login",
                    "postData": { "mimeType": "application/json", "text": "{\"u\":\"a\"}" }
                }, "response": { "status": 201 } }
            ] }
        });
        let apis = parse_har(&doc).expect("parsed");
        // /users GET 去重为 1 条 + /login POST = 2
        assert_eq!(apis.len(), 2);
        let users = apis.iter().find(|a| a.path == "/users").expect("users");
        assert_eq!(users.module.as_deref(), Some("api.example.com"));
        assert_eq!(users.spec["requestQuery"][0]["name"], "page");
        // 伪头被过滤,仅保留 Accept
        let hs = users.spec["requestHeaders"].as_array().unwrap();
        assert_eq!(hs.len(), 1);
        assert_eq!(hs[0]["name"], "Accept");

        let login = apis.iter().find(|a| a.path == "/login").expect("login");
        assert_eq!(login.method, "POST");
        assert_eq!(login.spec["bodyType"], "json");
        assert_eq!(login.case_assertions[0]["args"], 201); // 响应状态码
        assert!(login.case_body.is_some());
    }

    #[test]
    fn missing_entries_is_bad_import() {
        let err = parse_har(&json!({"log": {}})).unwrap_err();
        assert!(matches!(err, ApiDefinitionError::BadImport(_)));
    }
}
