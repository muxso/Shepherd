use serde_json::Value;

use crate::domain::error::ApiDefinitionError;
use crate::domain::import::{
    body_type_of, kv, path_and_query, simple_spec, status_assertions, ImportedApi,
};

pub fn parse_metersphere(doc: &Value) -> Result<Vec<ImportedApi>, ApiDefinitionError> {
    let list = locate_list(doc).ok_or_else(|| {
        ApiDefinitionError::BadImport("metersphere: interface list not found".into())
    })?;

    let mut out = Vec::new();
    for item in list {
        if let Some(api) = item_to_api(item) {
            out.push(api);
        }
    }
    if out.is_empty() {
        return Err(ApiDefinitionError::BadImport("metersphere: no valid interface found".into()));
    }
    Ok(out)
}

fn locate_list(doc: &Value) -> Option<&Vec<Value>> {
    if let Some(arr) = doc.as_array() {
        return Some(arr);
    }
    for key in ["data", "apiDefinitions", "apis", "apiDefinitionList"] {
        if let Some(arr) = doc.get(key).and_then(|v| v.as_array()) {
            return Some(arr);
        }
    }
    None
}

fn item_to_api(item: &Value) -> Option<ImportedApi> {
    let method = item.get("method").and_then(|v| v.as_str()).unwrap_or("GET").to_uppercase();
    let raw_path = item
        .get("path")
        .or_else(|| item.get("url"))
        .or_else(|| item.get("uri"))
        .and_then(|v| v.as_str())?;
    let (path, url_query) = path_and_query(raw_path);

    let request = item.get("request").map(resolve_request);
    let request = request.as_ref();

    let mut headers = Vec::new();
    let mut query: Vec<Value> = url_query.iter().map(|(k, v)| kv(k, v, "")).collect();
    let mut body_text = String::new();

    if let Some(req) = request {
        if let Some(hs) = req.get("headers").and_then(|v| v.as_array()) {
            for h in hs {
                if !enabled(h) {
                    continue;
                }
                if let Some(k) = h.get("name").or_else(|| h.get("key")).and_then(|v| v.as_str()) {
                    let v = h.get("value").and_then(|v| v.as_str()).unwrap_or("");
                    let d = h.get("description").and_then(|v| v.as_str()).unwrap_or("");
                    headers.push(kv(k, v, d));
                }
            }
        }
        for field in ["arguments", "query", "queryString"] {
            if let Some(qs) = req.get(field).and_then(|v| v.as_array()) {
                for q in qs {
                    if !enabled(q) {
                        continue;
                    }
                    if let Some(k) = q.get("name").or_else(|| q.get("key")).and_then(|v| v.as_str())
                    {
                        let v = q.get("value").and_then(|v| v.as_str()).unwrap_or("");
                        query.push(kv(k, v, ""));
                    }
                }
            }
        }
        body_text = body_of(req.get("body"));
    }

    let body_type = body_type_of(&body_text).to_string();
    let name = item
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("{method} {path}"));
    let description = item.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let module = module_of(item);

    let case_body = if matches!(method.as_str(), "POST" | "PUT" | "PATCH") && !body_text.is_empty()
    {
        Some(body_text.clone())
    } else {
        None
    };

    Some(ImportedApi {
        name,
        method,
        path,
        spec: simple_spec(
            description,
            headers,
            query,
            Vec::new(),
            &body_type,
            &body_text,
            Vec::new(),
        ),
        case_assertions: status_assertions(200),
        case_body,
        module,
    })
}

fn resolve_request(req: &Value) -> Value {
    match req {
        Value::String(s) => {
            serde_json::from_str(s).unwrap_or_else(|_| Value::Object(Default::default()))
        }
        other => other.clone(),
    }
}

fn body_of(body: Option<&Value>) -> String {
    let Some(body) = body else { return String::new() };
    match body {
        Value::String(s) => s.clone(),
        Value::Object(_) => {
            for f in ["raw", "json", "text", "data"] {
                if let Some(s) = body.get(f).and_then(|v| v.as_str()) {
                    return s.to_string();
                }
            }
            if let Some(j) = body.get("json").filter(|v| v.is_object() || v.is_array()) {
                return serde_json::to_string_pretty(j).unwrap_or_default();
            }
            String::new()
        }
        _ => String::new(),
    }
}

fn module_of(item: &Value) -> Option<String> {
    if let Some(p) = item.get("modulePath").and_then(|v| v.as_str()) {
        let seg = p.trim_start_matches('/').split('/').next().unwrap_or("").trim();
        if !seg.is_empty() {
            return Some(seg.to_string());
        }
    }
    item.get("moduleName")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn enabled(v: &Value) -> bool {
    v.get("enable").or_else(|| v.get("enabled")).and_then(|x| x.as_bool()).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_data_list_with_string_request() {
        let doc = json!({
            "projectName": "demo",
            "data": [
                {
                    "name": "login",
                    "method": "post",
                    "path": "/login",
                    "modulePath": "/auth/login-state",
                    "request": "{\"headers\":[{\"name\":\"X-Token\",\"value\":\"t\",\"enable\":true}],\"arguments\":[{\"name\":\"from\",\"value\":\"web\"}],\"body\":{\"raw\":\"{\\\"u\\\":\\\"a\\\"}\"}}"
                },
                {
                    "name": "list users",
                    "method": "GET",
                    "path": "/users?page=1",
                    "moduleName": "users"
                }
            ]
        });
        let apis = parse_metersphere(&doc).expect("parsed");
        assert_eq!(apis.len(), 2);
        let login = apis.iter().find(|a| a.path == "/login").expect("login");
        assert_eq!(login.method, "POST");
        assert_eq!(login.module.as_deref(), Some("auth"));
        assert_eq!(login.spec["requestHeaders"][0]["name"], "X-Token");
        assert_eq!(login.spec["requestQuery"][0]["name"], "from");
        assert_eq!(login.spec["bodyType"], "json");
        assert!(login.case_body.as_deref().unwrap().contains("\"u\""));

        let users = apis.iter().find(|a| a.path == "/users").expect("users");
        assert_eq!(users.module.as_deref(), Some("users"));
        assert_eq!(users.spec["requestQuery"][0]["name"], "page");
    }

    #[test]
    fn document_root_array_is_accepted() {
        let doc = json!([{ "name": "ping", "method": "GET", "path": "/ping" }]);
        let apis = parse_metersphere(&doc).expect("parsed");
        assert_eq!(apis.len(), 1);
        assert_eq!(apis[0].path, "/ping");
    }

    #[test]
    fn no_list_is_bad_import() {
        let err = parse_metersphere(&json!({"foo": 1})).unwrap_err();
        assert!(matches!(err, ApiDefinitionError::BadImport(_)));
    }
}
