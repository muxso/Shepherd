//! Mock 聚合。对某接口定义的挡板配置:匹配规则(JSON)+ 响应。

use crate::domain::error::ApiDefinitionError;

/// 创建 Mock 的入站请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewApiMock {
    pub api_definition_id: String,
    pub name: String,
    pub match_rule: serde_json::Value,
    pub response_status: i32,
    pub response_body: Option<String>,
    pub enabled: bool,
}

impl NewApiMock {
    /// 校验:name 非空(trim);response_status 在 100..=599;match_rule 缺省为 `{}`。
    pub fn new(
        api_definition_id: &str,
        name: &str,
        match_rule: serde_json::Value,
        response_status: i32,
        response_body: Option<String>,
        enabled: bool,
    ) -> Result<Self, ApiDefinitionError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiDefinitionError::EmptyName);
        }
        if !(100..=599).contains(&response_status) {
            return Err(ApiDefinitionError::BadResponseStatus(response_status));
        }
        // 匹配规则缺省给一个空对象,保证下游可直接当对象处理。
        let match_rule = if match_rule.is_null() {
            serde_json::json!({})
        } else {
            match_rule
        };
        Ok(Self {
            api_definition_id: api_definition_id.to_string(),
            name: name.to_string(),
            match_rule,
            response_status,
            response_body,
            enabled,
        })
    }
}

/// Mock 聚合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiMock {
    pub id: String,
    pub api_definition_id: String,
    pub name: String,
    pub match_rule: serde_json::Value,
    pub response_status: i32,
    pub response_body: Option<String>,
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_mock_ok() {
        let m = NewApiMock::new(
            "def1",
            " 挡板 ",
            serde_json::json!({"path": "/x"}),
            200,
            Some("{}".into()),
            true,
        )
        .expect("ok");
        assert_eq!(m.name, "挡板");
        assert_eq!(m.response_status, 200);
        assert!(m.enabled);
    }

    #[test]
    fn new_mock_defaults_match_rule_to_object() {
        let m = NewApiMock::new("d", "n", serde_json::Value::Null, 404, None, false)
            .expect("ok");
        assert_eq!(m.match_rule, serde_json::json!({}));
    }

    #[test]
    fn new_mock_rejects_blank_name() {
        let err = NewApiMock::new("d", " ", serde_json::json!({}), 200, None, true).unwrap_err();
        assert_eq!(err, ApiDefinitionError::EmptyName);
    }

    #[test]
    fn new_mock_rejects_status_out_of_range() {
        let err = NewApiMock::new("d", "n", serde_json::json!({}), 99, None, true).unwrap_err();
        assert_eq!(err, ApiDefinitionError::BadResponseStatus(99));
        let err = NewApiMock::new("d", "n", serde_json::json!({}), 600, None, true).unwrap_err();
        assert_eq!(err, ApiDefinitionError::BadResponseStatus(600));
    }
}
