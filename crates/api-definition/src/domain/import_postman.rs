//! Postman Collection v2.x 导入:把 collection 的 `item` 树摊平成一批待建接口。
//!
//! 纯函数、零 IO。递归遍历 `item`(文件夹可嵌套):文件夹名作为模块(取最近的祖先文件夹),
//! 请求项摊平为 [`ImportedApi`]。解析 method / url(对象或字符串)/ header / query / body(raw)。
//! 与 OpenAPI 导入对齐:URL 取相对路径(执行时由环境 baseUrl 拼接),默认用例带状态码断言。

use serde_json::Value;

use crate::domain::error::ApiDefinitionError;
use crate::domain::import::{
    body_type_of, kv, path_and_query, simple_spec, status_assertions, ImportedApi,
};

/// 解析 Postman Collection(v2.x)。无 `item` 数组或其中无任何请求时报 `BadImport`。
pub fn parse_postman(doc: &Value) -> Result<Vec<ImportedApi>, ApiDefinitionError> {
    let items = doc
        .get("item")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiDefinitionError::BadImport("postman: 缺少 `item` 数组".into()))?;

    let mut out = Vec::new();
    for it in items {
        walk(it, None, &mut out);
    }
    if out.is_empty() {
        return Err(ApiDefinitionError::BadImport("postman: 未找到任何请求".into()));
    }
    Ok(out)
}

/// 递归遍历一个 item:有 `item` 字段 → 文件夹(其名作为后代模块),否则 → 请求项。
fn walk(item: &Value, module: Option<&str>, out: &mut Vec<ImportedApi>) {
    if let Some(children) = item.get("item").and_then(|v| v.as_array()) {
        // 文件夹:其名作为子项的模块(顶层文件夹优先,仅取最近一层即可对齐 group_by_tag 的单层分组)。
        let folder = item.get("name").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty());
        let next = folder.or(module);
        for c in children {
            walk(c, next, out);
        }
        return;
    }
    if let Some(api) = request_to_api(item, module) {
        out.push(api);
    }
}

/// 把一个请求项转成 ImportedApi;无 request/无法定位路径时返回 None(跳过)。
fn request_to_api(item: &Value, module: Option<&str>) -> Option<ImportedApi> {
    let req = item.get("request")?;
    let method = req
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET")
        .to_uppercase();

    let (path, mut query) = url_of(req.get("url"))?;

    // 请求头(过滤 disabled)。
    let mut headers = Vec::new();
    if let Some(hs) = req.get("header").and_then(|v| v.as_array()) {
        for h in hs {
            if h.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
                continue;
            }
            let (Some(k), v) = (
                h.get("key").and_then(|v| v.as_str()),
                h.get("value").and_then(|v| v.as_str()).unwrap_or(""),
            ) else {
                continue;
            };
            let desc = h.get("description").and_then(|v| v.as_str()).unwrap_or("");
            headers.push(kv(k, v, desc));
        }
    }

    // 请求体:目前支持 raw(json/text)。urlencoded/formdata → 归并为查询参数兜底。
    let (body_type, body_text) = body_of(req.get("body"), &mut query);

    let query_kv: Vec<Value> = query.iter().map(|(k, v)| kv(k, v, "")).collect();
    let name = item
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("{method} {path}"));
    let description = item.get("request").and_then(|r| r.get("description")).and_then(|v| v.as_str()).unwrap_or("");

    let case_body = if matches!(method.as_str(), "POST" | "PUT" | "PATCH") && !body_text.is_empty() {
        Some(body_text.clone())
    } else {
        None
    };

    Some(ImportedApi {
        name,
        method,
        path: path.clone(),
        spec: simple_spec(description, headers, query_kv, Vec::new(), &body_type, &body_text, Vec::new()),
        case_assertions: status_assertions(200),
        case_body,
        module: module.map(str::to_string),
    })
}

/// 解析 url:对象形态优先(host/path/query 结构化),否则字符串形态。返回 `(路径, 查询)`。
fn url_of(url: Option<&Value>) -> Option<(String, Vec<(String, String)>)> {
    let url = url?;
    if let Some(s) = url.as_str() {
        return Some(path_and_query(s));
    }
    let obj = url.as_object()?;
    // path 为字符串数组(各段),拼为 /a/b;也可能直接给 raw。
    let path = match obj.get("path") {
        Some(Value::Array(segs)) => {
            let joined: Vec<String> = segs
                .iter()
                .map(|s| s.as_str().map(str::to_string).unwrap_or_else(|| seg_var(s)))
                .collect();
            format!("/{}", joined.join("/"))
        }
        Some(Value::String(s)) => path_and_query(s).0,
        _ => obj
            .get("raw")
            .and_then(|v| v.as_str())
            .map(|s| path_and_query(s).0)
            .unwrap_or_else(|| "/".to_string()),
    };
    let mut query = Vec::new();
    if let Some(qs) = obj.get("query").and_then(|v| v.as_array()) {
        for q in qs {
            if q.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
                continue;
            }
            if let Some(k) = q.get("key").and_then(|v| v.as_str()) {
                let v = q.get("value").and_then(|v| v.as_str()).unwrap_or("");
                query.push((k.to_string(), v.to_string()));
            }
        }
    }
    Some((path, query))
}

/// path 段可能是 `{ "value": ":id" }` 形态的变量段;取其 value。
fn seg_var(s: &Value) -> String {
    s.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string()
}

/// body:raw → (bodyType, 文本);urlencoded/formdata → 参数并入 query 并返回 ("none","")。
fn body_of(body: Option<&Value>, query: &mut Vec<(String, String)>) -> (String, String) {
    let Some(body) = body else { return ("none".to_string(), String::new()) };
    let mode = body.get("mode").and_then(|v| v.as_str()).unwrap_or("");
    match mode {
        "raw" => {
            let raw = body.get("raw").and_then(|v| v.as_str()).unwrap_or("");
            (body_type_of(raw).to_string(), raw.to_string())
        }
        "urlencoded" | "formdata" => {
            if let Some(arr) = body.get(mode).and_then(|v| v.as_array()) {
                for f in arr {
                    if f.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
                        continue;
                    }
                    if let Some(k) = f.get("key").and_then(|v| v.as_str()) {
                        let v = f.get("value").and_then(|v| v.as_str()).unwrap_or("");
                        query.push((k.to_string(), v.to_string()));
                    }
                }
            }
            ("none".to_string(), String::new())
        }
        _ => ("none".to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_nested_folders_and_url_object() {
        let doc = json!({
            "info": { "name": "demo" },
            "item": [
                { "name": "auth", "item": [
                    { "name": "登录", "request": {
                        "method": "post",
                        "header": [{ "key": "Content-Type", "value": "application/json" }],
                        "url": { "raw": "{{base}}/login?from=web", "path": ["login"], "query": [{ "key": "from", "value": "web" }] },
                        "body": { "mode": "raw", "raw": "{\"username\":\"admin\"}" }
                    } }
                ] },
                { "name": "ping", "request": { "method": "GET", "url": "https://h.example/api/ping" } }
            ]
        });
        let apis = parse_postman(&doc).expect("parsed");
        assert_eq!(apis.len(), 2);
        let login = apis.iter().find(|a| a.path == "/login").expect("login");
        assert_eq!(login.method, "POST");
        assert_eq!(login.module.as_deref(), Some("auth"));
        assert_eq!(login.spec["bodyType"], "json");
        assert!(login.case_body.as_deref().unwrap().contains("admin"));
        assert_eq!(login.spec["requestQuery"][0]["name"], "from");
        assert_eq!(login.spec["requestHeaders"][0]["name"], "Content-Type");
        // 状态码断言
        assert_eq!(login.case_assertions[0]["type"], "StatusIs");

        let ping = apis.iter().find(|a| a.path == "/api/ping").expect("ping");
        assert_eq!(ping.method, "GET");
        assert_eq!(ping.module, None); // 顶层请求,无文件夹
    }

    #[test]
    fn missing_item_is_bad_import() {
        let err = parse_postman(&json!({"info": {}})).unwrap_err();
        assert!(matches!(err, ApiDefinitionError::BadImport(_)));
    }

    #[test]
    fn empty_collection_is_bad_import() {
        let err = parse_postman(&json!({"item": []})).unwrap_err();
        assert!(matches!(err, ApiDefinitionError::BadImport(_)));
    }
}
