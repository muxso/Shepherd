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
    /// 标签 JSON 数组;默认空。
    pub tags: serde_json::Value,
    /// 自定义响应头 JSON 数组([{key,value}]);默认空。
    pub response_headers: serde_json::Value,
    /// 响应延时(毫秒);默认 0。负值回落 0。
    pub response_delay_ms: i32,
    /// 是否跟随 API 定义返回(开启时忽略 response_body,返回定义示例);默认 false。
    pub follow_definition: bool,
    /// 创建人 user_id(由应用层注入);默认空。
    pub created_by: String,
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
            tags: serde_json::Value::Array(Vec::new()),
            response_headers: serde_json::Value::Array(Vec::new()),
            response_delay_ms: 0,
            follow_definition: false,
            created_by: String::new(),
        })
    }

    /// 设置创建人(链式)。
    pub fn with_created_by(mut self, user_id: &str) -> Self {
        self.created_by = user_id.to_string();
        self
    }

    /// 附加 Mock 扩展项:标签 / 响应头 / 响应延时 / 跟随定义。
    /// tags、response_headers 非数组回落空数组;延时取 max(0, ms)。
    pub fn with_extras(
        mut self,
        tags: serde_json::Value,
        response_headers: serde_json::Value,
        response_delay_ms: i32,
        follow_definition: bool,
    ) -> Self {
        self.tags = if tags.is_array() { tags } else { serde_json::Value::Array(Vec::new()) };
        self.response_headers = if response_headers.is_array() {
            response_headers
        } else {
            serde_json::Value::Array(Vec::new())
        };
        self.response_delay_ms = response_delay_ms.max(0);
        self.follow_definition = follow_definition;
        self
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
    pub tags: serde_json::Value,
    pub response_headers: serde_json::Value,
    pub response_delay_ms: i32,
    pub follow_definition: bool,
    /// 创建人 user_id(审计列;见 0057 迁移)。读侧回填,创建时由应用层注入。
    pub created_by: String,
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
    fn with_extras_clamps_delay_and_defaults_arrays() {
        let m = NewApiMock::new("d", "n", serde_json::json!({}), 200, None, true)
            .expect("ok")
            .with_extras(serde_json::json!(["t"]), serde_json::json!("bad"), -5, true);
        assert_eq!(m.tags, serde_json::json!(["t"]));
        assert_eq!(m.response_headers, serde_json::json!([]));
        assert_eq!(m.response_delay_ms, 0);
        assert!(m.follow_definition);
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
