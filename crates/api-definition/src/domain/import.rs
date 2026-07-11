use crate::domain::error::ApiDefinitionError;
use serde_json::{json, Map, Value};

#[derive(Debug, Clone)]
pub struct ImportedApi {
    pub name: String,
    pub method: String,
    pub path: String,
    pub spec: Value,
    pub case_assertions: Value,
    pub case_body: Option<String>,
    pub module: Option<String>,
}

const HTTP_METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "head", "options"];
const MAX_DEPTH: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    Openapi,
    Postman,
    Har,
    Jmeter,
    Metersphere,
}

impl ImportFormat {
    pub fn from_source(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "postman" => Self::Postman,
            "har" => Self::Har,
            "jmeter" | "jmx" => Self::Jmeter,
            "metersphere" | "ms" => Self::Metersphere,
            _ => Self::Openapi,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Openapi => "openapi",
            Self::Postman => "postman",
            Self::Har => "har",
            Self::Jmeter => "jmeter",
            Self::Metersphere => "metersphere",
        }
    }
}

pub fn parse_import(
    format: ImportFormat,
    doc: &Value,
) -> Result<Vec<ImportedApi>, ApiDefinitionError> {
    match format {
        ImportFormat::Openapi => parse_openapi(doc),
        ImportFormat::Postman => super::import_postman::parse_postman(doc),
        ImportFormat::Har => super::import_har::parse_har(doc),
        ImportFormat::Jmeter => {
            let xml = doc.as_str().ok_or_else(|| {
                ApiDefinitionError::BadImport("jmeter 导入需原始 .jmx 文本".into())
            })?;
            super::import_jmeter::parse_jmeter(xml)
        }
        ImportFormat::Metersphere => super::import_metersphere::parse_metersphere(doc),
    }
}

pub(crate) fn simple_spec(
    description: &str,
    headers: Vec<Value>,
    query: Vec<Value>,
    rest: Vec<Value>,
    body_type: &str,
    request_body: &str,
    responses: Vec<Value>,
) -> Value {
    json!({
        "description": description,
        "tags": [],
        "requestHeaders": headers,
        "requestQuery": query,
        "restParams": rest,
        "bodyType": body_type,
        "requestBody": request_body,
        "bodySchema": [],
        "responses": responses,
        "auth": { "type": "none" },
    })
}

pub(crate) fn kv(name: &str, value: &str, desc: &str) -> Value {
    json!({ "name": name, "value": value, "desc": desc })
}

pub(crate) fn status_assertions(status: u16) -> Value {
    json!([{ "type": "StatusIs", "args": status }])
}

pub(crate) fn body_type_of(raw: &str) -> &'static str {
    if raw.trim().is_empty() {
        "none"
    } else if serde_json::from_str::<Value>(raw).is_ok() {
        "json"
    } else {
        "raw"
    }
}

pub(crate) fn path_and_query(url: &str) -> (String, Vec<(String, String)>) {
    let url = url.trim();
    let after_host = match url.find("://") {
        Some(i) => {
            let rest = &url[i + 3..];
            match rest.find('/') {
                Some(j) => &rest[j..],
                None => "/",
            }
        }
        None => url,
    };
    let (path, qs) = match after_host.split_once('?') {
        Some((p, q)) => (p, q),
        None => (after_host, ""),
    };
    let path = if path.is_empty() { "/".to_string() } else { path.to_string() };
    let mut query = Vec::new();
    for pair in qs.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        query.push((k.replace('+', " "), v.replace('+', " ")));
    }
    (path, query)
}

pub fn parse_openapi(doc: &Value) -> Result<Vec<ImportedApi>, ApiDefinitionError> {
    let base_path = doc
        .get("basePath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_end_matches('/')
        .to_string();

    let paths = doc
        .get("paths")
        .and_then(|v| v.as_object())
        .ok_or_else(|| ApiDefinitionError::BadImport("missing `paths` object".into()))?;

    let mut out = Vec::new();
    let mut path_keys: Vec<&String> = paths.keys().collect();
    path_keys.sort();
    for path_key in path_keys {
        let Some(ops) = paths[path_key].as_object() else { continue };
        let full_path = format!("{base_path}{path_key}");
        for method in HTTP_METHODS {
            let Some(op) = ops.get(*method) else { continue };
            let name = op
                .get("summary")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .or_else(|| op.get("operationId").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{} {full_path}", method.to_uppercase()));

            let spec = build_spec(doc, op);
            let (status, business_field) = success_case(doc, op);
            let mut assertions = vec![json!({ "type": "StatusIs", "args": status })];
            if let Some(field) = business_field {
                assertions.push(json!({ "type": "BodyContains", "args": field }));
            }
            let case_body = if matches!(*method, "post" | "put" | "patch") {
                spec.get("requestBody")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            } else {
                None
            };

            let module = op
                .get("tags")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|t| t.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            out.push(ImportedApi {
                name,
                method: method.to_uppercase(),
                path: full_path.clone(),
                spec,
                case_assertions: Value::Array(assertions),
                case_body,
                module,
            });
        }
    }

    if out.is_empty() {
        return Err(ApiDefinitionError::BadImport("no operations found in `paths`".into()));
    }
    Ok(out)
}

fn build_spec(doc: &Value, op: &Value) -> Value {
    let mut headers = Vec::new();
    let mut query = Vec::new();
    let mut rest = Vec::new();
    if let Some(params) = op.get("parameters").and_then(|v| v.as_array()) {
        for p in params {
            if let Some((loc, kv)) = kv_from_param(doc, p) {
                match loc.as_str() {
                    "header" => headers.push(kv),
                    "query" => query.push(kv),
                    "path" => rest.push(kv),
                    _ => {}
                }
            }
        }
    }

    let (body_type, body_text, body_schema) = request_body(doc, op);
    let responses = responses_spec(doc, op);
    let description = op
        .get("description")
        .or_else(|| op.get("summary"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tags = op.get("tags").cloned().unwrap_or_else(|| json!([]));

    json!({
        "description": description,
        "tags": tags,
        "requestHeaders": headers,
        "requestQuery": query,
        "restParams": rest,
        "bodyType": body_type,
        "requestBody": body_text,
        "bodySchema": body_schema,
        "responses": responses,
        "auth": { "type": "none" },
    })
}

fn kv_from_param(doc: &Value, p: &Value) -> Option<(String, Value)> {
    let p = resolve(doc, p, 0);
    let name = p.get("name")?.as_str()?.to_string();
    let loc = p.get("in")?.as_str()?.to_string();
    let required = p.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
    let ty = p
        .get("schema")
        .map(|s| schema_type(&resolve(doc, s, 0)))
        .unwrap_or_else(|| "string".to_string());
    let d = p.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let prefix = if required { "必填" } else { "选填" };
    let desc =
        if d.is_empty() { format!("{prefix} · {ty}") } else { format!("{prefix} · {ty} · {d}") };
    Some((loc, json!({ "name": name, "value": "", "desc": desc })))
}

fn request_body(doc: &Value, op: &Value) -> (String, String, Vec<Value>) {
    let schema = op
        .get("requestBody")
        .and_then(|b| b.get("content"))
        .and_then(|c| c.get("application/json"))
        .and_then(|j| j.get("schema"));
    match schema {
        Some(s) => {
            let text = serde_json::to_string_pretty(&example(doc, s, 0)).unwrap_or_default();
            ("json".to_string(), text, schema_nodes(doc, s, 0))
        }
        None => ("none".to_string(), String::new(), Vec::new()),
    }
}

fn responses_spec(doc: &Value, op: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let Some(resps) = op.get("responses").and_then(|v| v.as_object()) else { return out };
    let mut keys: Vec<&String> = resps.keys().collect();
    keys.sort();
    for k in keys {
        let status: u16 = k.parse().unwrap_or(0);
        let r = &resps[k];
        let body = match r
            .get("content")
            .and_then(|c| c.get("application/json"))
            .and_then(|j| j.get("schema"))
        {
            Some(s) => serde_json::to_string_pretty(&example(doc, s, 0)).unwrap_or_default(),
            None => r.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        };
        out.push(json!({ "status": status, "body": body }));
    }
    out
}

fn success_case(doc: &Value, op: &Value) -> (u16, Option<String>) {
    let Some(resps) = op.get("responses").and_then(|v| v.as_object()) else { return (200, None) };
    let mut codes: Vec<u16> = resps
        .keys()
        .filter_map(|k| k.parse::<u16>().ok())
        .filter(|c| (200..300).contains(c))
        .collect();
    codes.sort();
    let Some(&status) = codes.first() else { return (200, None) };

    let field = resps
        .get(&status.to_string())
        .and_then(|r| r.get("content"))
        .and_then(|c| c.get("application/json"))
        .and_then(|j| j.get("schema"))
        .and_then(|schema| {
            let s = resolve(doc, schema, 0);
            let props = s.get("properties").and_then(|v| v.as_object())?.clone();
            let required: Vec<String> = s
                .get("required")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            required.first().cloned().or_else(|| props.keys().next().cloned())
        });
    (status, field)
}

fn resolve(doc: &Value, schema: &Value, depth: u8) -> Value {
    if depth > MAX_DEPTH {
        return schema.clone();
    }
    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        return match resolve_pointer(doc, r) {
            Some(target) => resolve(doc, &target, depth + 1),
            None => Value::Object(Map::new()),
        };
    }
    if let Some(all) = schema.get("allOf").and_then(|v| v.as_array()) {
        let mut props = Map::new();
        let mut required = Vec::new();
        for sub in all {
            let r = resolve(doc, sub, depth + 1);
            if let Some(p) = r.get("properties").and_then(|v| v.as_object()) {
                for (k, v) in p {
                    props.insert(k.clone(), v.clone());
                }
            }
            if let Some(req) = r.get("required").and_then(|v| v.as_array()) {
                required.extend(req.iter().cloned());
            }
        }
        return json!({ "type": "object", "properties": props, "required": required });
    }
    schema.clone()
}

fn resolve_pointer(doc: &Value, r: &str) -> Option<Value> {
    let p = r.strip_prefix("#/")?;
    let mut cur = doc;
    for seg in p.split('/') {
        let seg = seg.replace("~1", "/").replace("~0", "~");
        cur = cur.get(&seg)?;
    }
    Some(cur.clone())
}

fn schema_type(schema: &Value) -> String {
    match schema.get("type") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .find(|s| *s != "null")
            .unwrap_or("string")
            .to_string(),
        _ if schema.get("properties").is_some() => "object".to_string(),
        _ => "string".to_string(),
    }
}

fn node_type(t: &str) -> &'static str {
    match t {
        "integer" => "integer",
        "number" => "number",
        "boolean" => "boolean",
        "object" => "object",
        "array" => "array",
        _ => "string",
    }
}

fn example(doc: &Value, schema: &Value, depth: u8) -> Value {
    if depth > MAX_DEPTH {
        return Value::Null;
    }
    let s = resolve(doc, schema, 0);
    if let Some(ex) = s.get("example") {
        return ex.clone();
    }
    if let Some(def) = s.get("default") {
        return def.clone();
    }
    match schema_type(&s).as_str() {
        "object" => {
            let mut m = Map::new();
            if let Some(props) = s.get("properties").and_then(|v| v.as_object()) {
                for (k, v) in props {
                    m.insert(k.clone(), example(doc, v, depth + 1));
                }
            }
            Value::Object(m)
        }
        "array" => {
            let item = s.get("items").map(|it| example(doc, it, depth + 1)).unwrap_or(Value::Null);
            Value::Array(vec![item])
        }
        "integer" | "number" => json!(0),
        "boolean" => json!(false),
        _ => json!(""),
    }
}

fn schema_nodes(doc: &Value, schema: &Value, depth: u8) -> Vec<Value> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    let s = resolve(doc, schema, 0);
    let required: Vec<String> = s
        .get("required")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let Some(props) = s.get("properties").and_then(|v| v.as_object()) else { return Vec::new() };

    let mut out = Vec::new();
    for (name, ps) in props {
        let rs = resolve(doc, ps, 0);
        let t = node_type(&schema_type(&rs));
        let d = ps
            .get("description")
            .or_else(|| rs.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let prefix = if required.contains(name) { "必填" } else { "选填" };
        let desc = if d.is_empty() { prefix.to_string() } else { format!("{prefix} · {d}") };
        let mut node = json!({ "name": name, "type": t, "value": "", "description": desc });
        if t == "object" {
            let children = schema_nodes(doc, &rs, depth + 1);
            if !children.is_empty() {
                node["children"] = json!(children);
            }
        } else if t == "array" {
            if let Some(items) = rs.get("items") {
                let ir = resolve(doc, items, 0);
                if schema_type(&ir) == "object" {
                    let children = schema_nodes(doc, &ir, depth + 1);
                    if !children.is_empty() {
                        node["children"] = json!(children);
                    }
                }
            }
        }
        out.push(node);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_openapi_3_paths_and_methods() {
        let doc = json!({
            "openapi": "3.0.0",
            "paths": {
                "/login": { "post": { "summary": "登录" } },
                "/users": {
                    "get": { "operationId": "listUsers" },
                    "post": {}
                }
            }
        });
        let apis = parse_openapi(&doc).expect("parsed");
        assert_eq!(apis.len(), 3);
        assert_eq!(
            (apis[0].name.as_str(), apis[0].method.as_str(), apis[0].path.as_str()),
            ("登录", "POST", "/login")
        );
        assert_eq!(apis[1].name, "listUsers");
        assert_eq!(apis[1].method, "GET");
        assert_eq!(apis[2].name, "POST /users");
    }

    #[test]
    fn first_tag_becomes_module() {
        let doc = json!({
            "paths": {
                "/a": { "get": { "summary": "a", "tags": ["auth", "x"] } },
                "/b": { "get": { "summary": "b" } }
            }
        });
        let apis = parse_openapi(&doc).expect("parsed");
        assert_eq!(apis[0].module.as_deref(), Some("auth"));
        assert_eq!(apis[1].module, None);
    }

    #[test]
    fn swagger_2_basepath_is_prefixed() {
        let doc = json!({
            "swagger": "2.0",
            "basePath": "/api/v1",
            "paths": { "/ping": { "get": { "summary": "ping" } } }
        });
        let apis = parse_openapi(&doc).expect("parsed");
        assert_eq!(apis[0].path, "/api/v1/ping");
    }

    #[test]
    fn missing_paths_is_bad_import() {
        let err = parse_openapi(&json!({"openapi": "3.0.0"})).unwrap_err();
        assert!(matches!(err, ApiDefinitionError::BadImport(_)));
    }

    #[test]
    fn empty_paths_is_bad_import() {
        let err = parse_openapi(&json!({"paths": {}})).unwrap_err();
        assert!(matches!(err, ApiDefinitionError::BadImport(_)));
    }

    #[test]
    fn non_method_keys_are_ignored() {
        let doc = json!({
            "paths": { "/x": { "parameters": [], "get": { "summary": "x" } } }
        });
        let apis = parse_openapi(&doc).expect("parsed");
        assert_eq!(apis.len(), 1);
        assert_eq!(apis[0].method, "GET");
    }

    #[test]
    fn parameters_become_spec_kv_with_required_in_desc() {
        let doc = json!({
            "paths": { "/u": { "get": { "summary": "u", "parameters": [
                { "name": "projectId", "in": "query", "required": true, "description": "项目", "schema": {"type": "string"} },
                { "name": "X-Token", "in": "header", "required": false, "schema": {"type": "string"} },
                { "name": "id", "in": "path", "required": true, "schema": {"type": "integer"} }
            ] } } }
        });
        let api = &parse_openapi(&doc).expect("parsed")[0];
        let q = api.spec["requestQuery"].as_array().unwrap();
        assert_eq!(q[0]["name"], "projectId");
        assert!(q[0]["desc"].as_str().unwrap().contains("必填"));
        assert!(q[0]["desc"].as_str().unwrap().contains("项目"));
        let h = api.spec["requestHeaders"].as_array().unwrap();
        assert!(h[0]["desc"].as_str().unwrap().contains("选填"));
        let r = api.spec["restParams"].as_array().unwrap();
        assert_eq!(r[0]["name"], "id");
    }

    #[test]
    fn request_body_ref_resolves_to_schema_and_case_body() {
        let doc = json!({
            "components": { "schemas": { "LoginBody": {
                "type": "object", "required": ["username"],
                "properties": { "username": {"type": "string"}, "remember": {"type": "boolean"} }
            } } },
            "paths": { "/login": { "post": { "summary": "登录",
                "requestBody": { "content": { "application/json": { "schema": { "$ref": "#/components/schemas/LoginBody" } } } },
                "responses": { "200": { "content": { "application/json": { "schema": {
                    "type": "object", "required": ["token"], "properties": { "token": {"type": "string"} }
                } } } } }
            } } }
        });
        let api = &parse_openapi(&doc).expect("parsed")[0];
        assert_eq!(api.spec["bodyType"], "json");
        let nodes = api.spec["bodySchema"].as_array().unwrap();
        let names: Vec<&str> = nodes.iter().map(|n| n["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"username") && names.contains(&"remember"));
        assert!(api.case_body.as_deref().unwrap().contains("username"));
        let asserts = api.case_assertions.as_array().unwrap();
        assert_eq!(asserts[0], json!({"type": "StatusIs", "args": 200}));
        assert_eq!(asserts[1], json!({"type": "BodyContains", "args": "token"}));
    }

    #[test]
    fn get_without_response_schema_only_status_assertion() {
        let doc = json!({
            "paths": { "/health": { "get": { "summary": "health",
                "responses": { "204": { "description": "ok" } } } } }
        });
        let api = &parse_openapi(&doc).expect("parsed")[0];
        let asserts = api.case_assertions.as_array().unwrap();
        assert_eq!(asserts.len(), 1);
        assert_eq!(asserts[0], json!({"type": "StatusIs", "args": 204}));
        assert!(api.case_body.is_none());
    }
}
