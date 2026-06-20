//! 响应体模板渲染(minijinja):命中规则后,把请求上下文注入响应 body 模板。
//!
//! 纯计算(无 IO),与匹配引擎同属 domain。注入的上下文:
//! - `method` / `path` / `body`(原始串)
//! - `query` / `headers`(map,header 键小写)
//! - `json`(请求体解析为 JSON,可 `{{ json.user.id }}` 导航;体非 JSON 则为 null)
//!
//! 内置函数 `uuid()` 生成随机 id(动态 mock 数据示例;`now()`/`fake_*` 可同样 add_function 扩展)。
//!
//! 不含 `{{`/`{%`/`{#` 的普通字符串走快路径原样返回(免起引擎)。

use minijinja::{context, Environment};
use thiserror::Error;

use crate::domain::MockRequest;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TemplateError {
    #[error("template render error: {0}")]
    Render(String),
}

/// 用请求上下文渲染响应 body 模板。非模板字符串原样返回。
pub fn render_body(template: &str, req: &MockRequest) -> Result<String, TemplateError> {
    if !template.contains("{{") && !template.contains("{%") && !template.contains("{#") {
        return Ok(template.to_string());
    }
    let json: serde_json::Value = req
        .body
        .as_deref()
        .and_then(|b| serde_json::from_str(b).ok())
        .unwrap_or(serde_json::Value::Null);

    let mut env = Environment::new();
    env.add_function("uuid", || uuid::Uuid::new_v4().to_string());

    env.render_str(
        template,
        context! {
            method => req.method,
            path => req.path,
            query => req.query,
            headers => req.headers,
            body => req.body,
            json => json,
        },
    )
    .map_err(|e| TemplateError::Render(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn req() -> MockRequest {
        MockRequest::normalized(
            "GET",
            "/users/42",
            BTreeMap::from([("status".into(), "paid".into())]),
            BTreeMap::from([("X-Trace".into(), "t-1".into())]),
            Some(r#"{"user":{"name":"Alice"}}"#.into()),
        )
    }

    #[test]
    fn plain_string_is_returned_as_is() {
        assert_eq!(render_body(r#"{"ok":true}"#, &req()).expect("ok"), r#"{"ok":true}"#);
    }

    #[test]
    fn interpolates_request_context() {
        let out = render_body(
            r#"{"path":"{{ path }}","status":"{{ query.status }}","name":"{{ json.user.name }}","trace":"{{ headers['x-trace'] }}"}"#,
            &req(),
        )
        .expect("ok");
        assert_eq!(
            out,
            r#"{"path":"/users/42","status":"paid","name":"Alice","trace":"t-1"}"#
        );
    }

    #[test]
    fn supports_control_flow() {
        let out = render_body(
            "{% if query.status == 'paid' %}PAID{% else %}OTHER{% endif %}",
            &req(),
        )
        .expect("ok");
        assert_eq!(out, "PAID");
    }

    #[test]
    fn uuid_function_produces_id() {
        let out = render_body(r#"{"id":"{{ uuid() }}"}"#, &req()).expect("ok");
        // 形如 {"id":"<36 字符 uuid>"}
        let id = out.trim_start_matches(r#"{"id":""#).trim_end_matches(r#""}"#);
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
    }

    #[test]
    fn invalid_template_is_error_not_panic() {
        assert!(render_body("{{ unclosed ", &req()).is_err());
    }
}
