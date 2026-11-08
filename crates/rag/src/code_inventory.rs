use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub method: String,
    pub path: String,
    pub file: String,
    pub line: usize,
    pub handler: String,
    pub evidence_document_id: String,
    #[serde(default)]
    pub request_model: serde_json::Value,
    #[serde(default)]
    pub response_models: serde_json::Value,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub endpoints: Vec<Endpoint>,
    pub limitations: Vec<String>,
    pub scanned_paths: Vec<String>,
}
pub fn reconstruct(chunks: &[String]) -> Result<BTreeMap<usize, String>, String> {
    let mut out = BTreeMap::new();
    for chunk in chunks {
        for line in chunk.lines() {
            let (label, body) = line.split_once(':').ok_or("code chunk lacks line references")?;
            let number = label
                .strip_prefix('L')
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|n| *n > 0)
                .ok_or("invalid code line reference")?;
            out.entry(number)
                .or_insert_with(String::new)
                .push_str(body.strip_prefix(' ').unwrap_or(body));
        }
    }
    if out.is_empty() {
        return Err("indexed file is empty".into());
    }
    if out.keys().enumerate().any(|(i, line)| i + 1 != *line) {
        return Err("indexed file has missing lines".into());
    }
    Ok(out)
}
fn model_refs(value: &serde_json::Value) -> serde_json::Value {
    fn walk(v: &serde_json::Value, out: &mut BTreeSet<String>, depth: usize) {
        if depth > 16 || out.len() >= 32 {
            return;
        }
        match v {
            serde_json::Value::Object(o) => {
                if let Some(r) = o.get("$ref").and_then(serde_json::Value::as_str) {
                    out.insert(r.chars().take(500).collect());
                }
                if let Some(t) = o.get("type").and_then(serde_json::Value::as_str) {
                    out.insert(format!("type:{t}"));
                }
                for v in o.values() {
                    walk(v, out, depth + 1);
                }
            }
            serde_json::Value::Array(a) => {
                for v in a {
                    walk(v, out, depth + 1);
                }
            }
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(value, &mut out, 0);
    serde_json::json!(out)
}
fn openapi(file: &str, document: &str, lines: &BTreeMap<usize, String>) -> Vec<Endpoint> {
    let content = lines.values().cloned().collect::<Vec<_>>().join("\n");
    let mut endpoints = Vec::new();
    if let Ok(spec) = serde_json::from_str::<serde_json::Value>(&content) {
        if spec.get("openapi").is_none() && spec.get("swagger").is_none() {
            return endpoints;
        }
        for (path, operations) in spec["paths"].as_object().into_iter().flatten() {
            for (method, operation) in operations.as_object().into_iter().flatten() {
                if !matches!(
                    method.as_str(),
                    "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
                ) {
                    continue;
                }
                let line = lines
                    .iter()
                    .find(|(_, line)| line.contains(&format!("\"{path}\"")))
                    .map(|(n, _)| *n)
                    .unwrap_or(1);
                endpoints.push(Endpoint {
                    method: method.to_uppercase(),
                    path: format!("{}{}", spec["basePath"].as_str().unwrap_or(""), path),
                    file: file.into(),
                    line,
                    handler: operation["operationId"].as_str().unwrap_or("").into(),
                    evidence_document_id: document.into(),
                    request_model: model_refs(&serde_json::json!([
                        operation["requestBody"],
                        operation["parameters"],
                        operations["parameters"]
                    ])),
                    response_models: model_refs(&operation["responses"]),
                });
            }
        }
        return endpoints;
    }
    if !content.lines().any(|line| line.starts_with("openapi:") || line.starts_with("swagger:")) {
        return endpoints;
    }
    let mut in_paths = false;
    let mut path = String::new();
    let mut indent = 0usize;
    for (&line, text) in lines {
        let trim = text.trim();
        let spaces = text.len() - text.trim_start().len();
        if trim == "paths:" && spaces == 0 {
            in_paths = true;
            continue;
        }
        if !in_paths {
            continue;
        }
        if spaces == 0 && !trim.is_empty() && !trim.starts_with('#') {
            break;
        }
        let key = trim.trim_end_matches(':').trim_matches(['\"', '\'']);
        if trim.ends_with(':') && key.starts_with('/') {
            path = key.into();
            indent = spaces;
            continue;
        }
        if !path.is_empty()
            && spaces > indent
            && trim.ends_with(':')
            && matches!(key, "get" | "post" | "put" | "patch" | "delete" | "head" | "options")
        {
            endpoints.push(Endpoint {
                method: key.to_uppercase(),
                path: path.clone(),
                file: file.into(),
                line,
                handler: String::new(),
                evidence_document_id: document.into(),
                request_model: serde_json::Value::Null,
                response_models: serde_json::Value::Null,
            });
        }
    }
    endpoints
}
pub fn scan(files: &[(String, String, BTreeMap<usize, String>)]) -> Inventory {
    let spring =
        regex::Regex::new(r#"@(Get|Post|Put|Patch|Delete|Request)Mapping\s*(?:\(([^)]*)\))?"#)
            .expect("static route pattern");
    let literal = regex::Regex::new(r#"[\"']([^\"']*)[\"']"#).expect("static string pattern");
    let python=regex::Regex::new(r#"@(?:app|router|api|blueprint|bp)\.(get|post|put|patch|delete|route)\s*\(\s*[\"']([^\"']*)[\"']"#).expect("static Python pattern");
    let node=regex::Regex::new(r#"\b(?:app|router|server)\.(get|post|put|patch|delete|head|options|all)\s*\(\s*[\"'`]([^\"'`]*)[\"'`]"#).expect("static JavaScript pattern");
    let rust = regex::Regex::new(
        r#"\.route\(\s*\"([^\"]*)\"\s*,\s*(get|post|put|patch|delete|head|options)\s*\("#,
    )
    .expect("static Rust pattern");
    let go = regex::Regex::new(r#"\.(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\s*\(\s*\"([^\"]*)\""#)
        .expect("static Go pattern");
    let class =
        regex::Regex::new(r"\b(class|interface|object)\s+\w+").expect("static class pattern");
    let handler=regex::Regex::new(r"(?:def|fn|func|function)\s+(\w+)|(?:public|private|protected)\s+[\w<>?,.\[\] ]+\s+(\w+)\s*\(").expect("static handler pattern");
    let mut endpoints = Vec::new();
    let mut limits = BTreeSet::new();
    let mut paths = Vec::new();
    for (path, document, lines) in files {
        paths.push(path.clone());
        let openapi = openapi(path, document, lines);
        if !openapi.is_empty() {
            endpoints.extend(openapi);
            limits.insert(format!("{path}：OpenAPI 声明已提取，仍需与实现、网关和测试资产核对"));
        }
        let mut prefix = String::new();
        let mut pending_prefix: Option<String> = None;
        let mut source_routes = 0usize;
        let ordered: Vec<_> = lines.iter().collect();
        for (index, (&number, text)) in ordered.iter().enumerate() {
            let trim = text.trim();
            if trim.starts_with("//")
                || trim.starts_with('#') && !trim.starts_with("#[")
                || trim.starts_with('*')
            {
                continue;
            }
            let joined = ordered
                .iter()
                .skip(index)
                .take(6)
                .map(|(_, line)| line.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let mut routes = Vec::<(String, String)>::new();
            if let Some(c) = spring.captures(trim) {
                let args = c.get(2).map(|m| m.as_str()).unwrap_or("");
                let strings: Vec<_> =
                    literal.captures_iter(args).map(|c| c[1].to_string()).collect();
                let route = strings.first().cloned().unwrap_or_default();
                if &c[1] == "Request"
                    && ordered.iter().skip(index + 1).take(6).any(|(_, line)| class.is_match(line))
                {
                    pending_prefix = Some(route);
                    continue;
                }
                let method = if &c[1] == "Request" {
                    ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]
                        .into_iter()
                        .find(|m| args.contains(&format!("RequestMethod.{m}")))
                        .unwrap_or("ANY")
                        .to_string()
                } else {
                    c[1].to_uppercase()
                };
                if !args.is_empty() && strings.is_empty() {
                    limits.insert(format!("{path}:{number} 路由使用动态值，待核对"));
                    continue;
                }
                if strings.len() > 1 {
                    limits.insert(format!("{path}:{number} 多路径映射只提取首项，待补齐"));
                }
                routes.push((
                    method,
                    format!("{}/{}", prefix.trim_end_matches('/'), route.trim_start_matches('/')),
                ));
            }
            if class.is_match(trim) {
                prefix = pending_prefix.take().unwrap_or_default();
            }
            if let Some(c) = python.captures(trim) {
                let method = c[1].to_uppercase();
                if method == "ROUTE" {
                    limits.insert(format!("{path}:{number} 装饰器路由方法待确认"));
                }
                routes.push((if method == "ROUTE" { "ANY".into() } else { method }, c[2].into()));
            }
            if let Some(c) = node.captures(trim) {
                if c[2].contains("${") {
                    limits.insert(format!("{path}:{number} 动态 JavaScript 路由待确认"));
                } else {
                    routes.push((
                        if &c[1] == "all" { "ANY".into() } else { c[1].to_uppercase() },
                        c[2].into(),
                    ));
                }
            }
            if let Some(c) = rust.captures(trim) {
                routes.push((c[2].to_uppercase(), c[1].into()));
                if joined.contains(".post(")
                    || joined.contains(".put(")
                    || joined.contains(".delete(")
                {
                    limits.insert(format!("{path}:{number} Rust 链式路由方法需复核"));
                }
            }
            if let Some(c) = go.captures(trim) {
                routes.push((c[1].into(), c[2].into()));
            }
            let name = handler
                .captures(&joined)
                .and_then(|c| c.get(1).or_else(|| c.get(2)))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            for (method, route) in routes {
                if !route.starts_with('/') {
                    limits.insert(format!("{path}:{number} 路由前缀待确认"));
                }
                source_routes += 1;
                endpoints.push(Endpoint {
                    method,
                    path: route,
                    file: path.clone(),
                    line: number,
                    handler: name.clone(),
                    evidence_document_id: document.clone(),
                    request_model: serde_json::Value::Null,
                    response_models: serde_json::Value::Null,
                });
            }
        }
        if source_routes > 0 {
            limits.insert(format!("{path}：网关前缀、动态注册和跨服务调用仍需人工确认"));
        }
    }
    endpoints.sort_by(|a, b| {
        (&a.file, a.line, &a.method, &a.path).cmp(&(&b.file, b.line, &b.method, &b.path))
    });
    endpoints.dedup_by(|a, b| {
        a.file == b.file && a.line == b.line && a.method == b.method && a.path == b.path
    });
    if endpoints.is_empty() {
        limits.insert("当前静态规则未识别 HTTP 入口；需确认仓库类型或补充框架规则".into());
    }
    Inventory { endpoints, limitations: limits.into_iter().collect(), scanned_paths: paths }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_lines_and_split_lines_are_preserved() {
        let chunks = vec!["L1: abc\nL1: def\nL2: next\n".into()];
        let lines = reconstruct(&chunks).expect("valid chunks");
        assert_eq!(lines[&1], "abcdef");
        assert!(reconstruct(&["L2: missing first".into()]).is_err());
    }
    #[test]
    fn scans_routes_with_java_class_prefix_and_marks_gaps() {
        let lines = BTreeMap::from([
            (1, "@RequestMapping(\"/orders\")".into()),
            (2, "public class OrdersController {".into()),
            (3, "@PostMapping(\"/submit\")".into()),
            (4, "public Order submit() {".into()),
        ]);
        let r = scan(&[("src/Orders.java".into(), "doc".into(), lines)]);
        assert_eq!(r.endpoints.len(), 1);
        assert_eq!(r.endpoints[0].path, "/orders/submit");
        assert_eq!(r.endpoints[0].line, 3);
        assert!(!r.limitations.is_empty());
    }
    #[test]
    fn excludes_comments_and_never_guesses_dynamic_routes() {
        let lines = BTreeMap::from([
            (1, "// router.get(\"/not-real\", handler)".into()),
            (2, "router.get(`/orders/${id}`, handler)".into()),
            (3, "@app.post(\"/orders\")".into()),
            (4, "def create_order():".into()),
        ]);
        let r = scan(&[("app.py".into(), "doc".into(), lines)]);
        assert_eq!(r.endpoints.len(), 1);
        assert_eq!(r.endpoints[0].method, "POST");
        assert_eq!(r.endpoints[0].handler, "create_order");
    }
}

#[cfg(test)]
mod openapi_tests {
    use super::*;
    #[test]
    fn declared_json_paths_keep_model_refs_and_source_lines() {
        let text = r##"{"openapi":"3.0.3","paths":{"/orders":{"post":{"operationId":"createOrder","requestBody":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Order"}}}},"responses":{"200":{"content":{"application/json":{"schema":{"type":"object"}}}}}}}}}"##;
        let input = vec![("openapi.json".into(), "doc".into(), BTreeMap::from([(1, text.into())]))];
        let scan = scan(&input);
        assert_eq!(scan.endpoints[0].path, "/orders");
        assert_eq!(
            scan.endpoints[0].request_model,
            serde_json::json!(["#/components/schemas/Order"])
        );
    }
    #[test]
    fn yaml_declarations_do_not_turn_schemas_into_paths() {
        let lines="openapi: 3.0.0\npaths:\n  /orders:\n    get:\n      responses: {}\ncomponents:\n  schemas:\n    /ignored:\n      post:".lines().enumerate().map(|(i,s)|(i+1,s.to_string())).collect();
        let scan = scan(&[("openapi.yaml".into(), "doc".into(), lines)]);
        assert_eq!(scan.endpoints.len(), 1);
        assert_eq!(scan.endpoints[0].method, "GET");
    }
}
