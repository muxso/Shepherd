//! 接口定义导入:把 OpenAPI 3.x / Swagger 2.0 文档解析成一批待建的接口。
//!
//! 纯函数、零 IO:只把文档 `paths` 下的每个 `路径 × HTTP 方法` 摊平成一条
//! [`ImportedApi`],由应用层逐条建为 [`super::NewApiDefinition`]。名称优先取
//! `summary` → `operationId` → `METHOD path`。Swagger 2.0 的 `basePath` 会拼到路径前。

use crate::domain::error::ApiDefinitionError;

/// 从导入文档摊平出的一条接口(协议固定 HTTP——OpenAPI/Swagger 描述的是 HTTP)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedApi {
    pub name: String,
    pub method: String,
    pub path: String,
}

const HTTP_METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "head", "options"];

/// 解析 OpenAPI 3.x / Swagger 2.0 文档。无法识别或 `paths` 缺失/为空时报 `BadImport`。
/// 返回按「路径、方法」稳定排序的接口列表(便于幂等与可预期的导入结果)。
pub fn parse_openapi(doc: &serde_json::Value) -> Result<Vec<ImportedApi>, ApiDefinitionError> {
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
    // BTreeMap 顺序:serde_json 的 Map 默认保留插入序;为稳定输出,显式按 key 排序。
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
            out.push(ImportedApi {
                name,
                method: method.to_uppercase(),
                path: full_path.clone(),
            });
        }
    }

    if out.is_empty() {
        return Err(ApiDefinitionError::BadImport("no operations found in `paths`".into()));
    }
    Ok(out)
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
        // 稳定排序:/login 在 /users 前;同路径按方法白名单序(get 在 post 前)
        assert_eq!(apis[0], ImportedApi { name: "登录".into(), method: "POST".into(), path: "/login".into() });
        assert_eq!(apis[1].name, "listUsers"); // operationId 兜底
        assert_eq!(apis[1].method, "GET");
        assert_eq!(apis[2].name, "POST /users"); // 无 summary/operationId → METHOD path
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
        // paths 下的 parameters / $ref 等非方法键不应被当成操作
        let doc = json!({
            "paths": { "/x": { "parameters": [], "get": { "summary": "x" } } }
        });
        let apis = parse_openapi(&doc).expect("parsed");
        assert_eq!(apis.len(), 1);
        assert_eq!(apis[0].method, "GET");
    }
}
