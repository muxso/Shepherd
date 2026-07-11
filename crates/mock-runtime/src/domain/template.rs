use minijinja::{context, Environment};
use thiserror::Error;

use crate::domain::MockRequest;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TemplateError {
    #[error("template render error: {0}")]
    Render(String),
}

fn register_functions(env: &mut Environment<'static>) {
    use fake::faker::internet::en::SafeEmail;
    use fake::faker::name::en::Name;
    use fake::Fake;

    env.add_function("uuid", || uuid::Uuid::new_v4().to_string());
    // 字符串,避免大整数精度问题。
    env.add_function("now", || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_else(|_| "0".to_string())
    });
    env.add_function("randint", |min: i64, max: i64| -> i64 {
        if min >= max {
            min
        } else {
            (min..max).fake()
        }
    });
    env.add_function("fake_name", || Name().fake::<String>());
    env.add_function("fake_email", || SafeEmail().fake::<String>());
}

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
    register_functions(&mut env);

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
        assert_eq!(out, r#"{"path":"/users/42","status":"paid","name":"Alice","trace":"t-1"}"#);
    }

    #[test]
    fn supports_control_flow() {
        let out =
            render_body("{% if query.status == 'paid' %}PAID{% else %}OTHER{% endif %}", &req())
                .expect("ok");
        assert_eq!(out, "PAID");
    }

    #[test]
    fn uuid_function_produces_id() {
        let out = render_body(r#"{"id":"{{ uuid() }}"}"#, &req()).expect("ok");
        let id = out.trim_start_matches(r#"{"id":""#).trim_end_matches(r#""}"#);
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
    }

    #[test]
    fn dynamic_functions_produce_data() {
        assert_eq!(render_body("{{ randint(1, 2) }}", &req()).expect("ok"), "1");
        let now = render_body("{{ now() }}", &req()).expect("ok");
        assert!(now.parse::<u64>().expect("num") > 0);
        assert!(render_body("{{ fake_email() }}", &req()).expect("ok").contains('@'));
        assert!(!render_body("{{ fake_name() }}", &req()).expect("ok").trim().is_empty());
    }

    #[test]
    fn invalid_template_is_error_not_panic() {
        assert!(render_body("{{ unclosed ", &req()).is_err());
    }
}
