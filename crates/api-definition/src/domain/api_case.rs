//! 接口用例聚合。挂在接口定义之下,带断言(JSON 数组)。

use crate::domain::error::{normalize_http_method, ApiDefinitionError};

/// 创建接口用例的入站请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewApiCase {
    pub api_definition_id: String,
    pub project_id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub body: Option<String>,
    pub assertions: serde_json::Value,
}

impl NewApiCase {
    /// 校验:name/url 非空(trim);method 在 HTTP 方法白名单内并规整大写;
    /// assertions 必须是 JSON 数组。
    pub fn new(
        api_definition_id: &str,
        project_id: &str,
        name: &str,
        method: &str,
        url: &str,
        body: Option<String>,
        assertions: serde_json::Value,
    ) -> Result<Self, ApiDefinitionError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiDefinitionError::EmptyName);
        }
        let url = url.trim();
        if url.is_empty() {
            return Err(ApiDefinitionError::EmptyUrl);
        }
        let method = normalize_http_method(method)?;
        if !assertions.is_array() {
            return Err(ApiDefinitionError::BadAssertions);
        }
        Ok(Self {
            api_definition_id: api_definition_id.to_string(),
            project_id: project_id.to_string(),
            name: name.to_string(),
            method,
            url: url.to_string(),
            body,
            assertions,
        })
    }
}

/// 接口用例聚合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiCase {
    pub id: String,
    pub api_definition_id: String,
    pub project_id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub body: Option<String>,
    pub assertions: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_case_ok_and_uppercases_method() {
        let c = NewApiCase::new(
            "def1",
            "p1",
            " 用例 ",
            "post",
            " /login ",
            Some("{}".into()),
            serde_json::json!([{"type": "status", "value": 200}]),
        )
        .expect("ok");
        assert_eq!(c.name, "用例");
        assert_eq!(c.method, "POST");
        assert_eq!(c.url, "/login");
    }

    #[test]
    fn new_case_rejects_blank_name() {
        let err = NewApiCase::new("d", "p", " ", "GET", "/x", None, serde_json::json!([]))
            .unwrap_err();
        assert_eq!(err, ApiDefinitionError::EmptyName);
    }

    #[test]
    fn new_case_rejects_blank_url() {
        let err = NewApiCase::new("d", "p", "n", "GET", "  ", None, serde_json::json!([]))
            .unwrap_err();
        assert_eq!(err, ApiDefinitionError::EmptyUrl);
    }

    #[test]
    fn new_case_rejects_unknown_method() {
        let err = NewApiCase::new("d", "p", "n", "FETCH", "/x", None, serde_json::json!([]))
            .unwrap_err();
        assert_eq!(err, ApiDefinitionError::UnknownMethod("FETCH".into()));
    }

    #[test]
    fn new_case_rejects_non_array_assertions() {
        let err = NewApiCase::new("d", "p", "n", "GET", "/x", None, serde_json::json!({}))
            .unwrap_err();
        assert_eq!(err, ApiDefinitionError::BadAssertions);
    }
}
