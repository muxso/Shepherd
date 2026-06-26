use crate::domain::error::{normalize_http_method, ApiDefinitionError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewApiCase {
    pub api_definition_id: String,
    pub project_id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub body: Option<String>,
    pub assertions: serde_json::Value,
    pub processors: serde_json::Value,
    pub priority: String,
    pub status: String,
    pub tags: serde_json::Value,
    pub headers: serde_json::Value,
    pub query_params: serde_json::Value,
    pub rest_params: serde_json::Value,
    pub auth: serde_json::Value,
}

const DEFAULT_PRIORITY: &str = "P0";
const DEFAULT_STATUS: &str = "进行中";

impl NewApiCase {
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
            processors: serde_json::Value::Array(Vec::new()),
            priority: DEFAULT_PRIORITY.to_string(),
            status: DEFAULT_STATUS.to_string(),
            tags: serde_json::Value::Array(Vec::new()),
            headers: serde_json::Value::Array(Vec::new()),
            query_params: serde_json::Value::Array(Vec::new()),
            rest_params: serde_json::Value::Array(Vec::new()),
            auth: serde_json::json!({}),
        })
    }

    pub fn with_processors(mut self, processors: serde_json::Value) -> Self {
        self.processors =
            if processors.is_array() { processors } else { serde_json::Value::Array(Vec::new()) };
        self
    }

    pub fn with_meta(mut self, priority: &str, status: &str, tags: serde_json::Value) -> Self {
        if !priority.trim().is_empty() {
            self.priority = priority.trim().to_string();
        }
        if !status.trim().is_empty() {
            self.status = status.trim().to_string();
        }
        self.tags = if tags.is_array() { tags } else { serde_json::Value::Array(Vec::new()) };
        self
    }

    pub fn with_headers(mut self, headers: serde_json::Value) -> Self {
        self.headers =
            if headers.is_array() { headers } else { serde_json::Value::Array(Vec::new()) };
        self
    }

    pub fn with_request(
        mut self,
        query_params: serde_json::Value,
        rest_params: serde_json::Value,
        auth: serde_json::Value,
    ) -> Self {
        self.query_params =
            if query_params.is_array() { query_params } else { serde_json::Value::Array(Vec::new()) };
        self.rest_params =
            if rest_params.is_array() { rest_params } else { serde_json::Value::Array(Vec::new()) };
        self.auth = if auth.is_object() { auth } else { serde_json::json!({}) };
        self
    }
}

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
    pub processors: serde_json::Value,
    pub priority: String,
    pub status: String,
    pub tags: serde_json::Value,
    pub headers: serde_json::Value,
    pub query_params: serde_json::Value,
    pub rest_params: serde_json::Value,
    pub auth: serde_json::Value,
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
    fn with_meta_trims_and_defaults() {
        let c = NewApiCase::new("d", "p", "n", "GET", "/x", None, serde_json::json!([]))
            .expect("ok")
            .with_meta("  P1 ", "  ", serde_json::json!(["smoke"]));
        assert_eq!(c.priority, "P1");
        assert_eq!(c.status, "进行中");
        assert_eq!(c.tags, serde_json::json!(["smoke"]));
    }

    #[test]
    fn with_meta_non_array_tags_fall_back_empty() {
        let c = NewApiCase::new("d", "p", "n", "GET", "/x", None, serde_json::json!([]))
            .expect("ok")
            .with_meta("", "已完成", serde_json::json!({"x": 1}))
            .with_headers(serde_json::json!("nope"));
        assert_eq!(c.priority, "P0");
        assert_eq!(c.status, "已完成");
        assert_eq!(c.tags, serde_json::json!([]));
        assert_eq!(c.headers, serde_json::json!([]));
    }

    #[test]
    fn with_request_keeps_arrays_and_object_falls_back() {
        let c = NewApiCase::new("d", "p", "n", "GET", "/x", None, serde_json::json!([]))
            .expect("ok")
            .with_request(serde_json::json!([{"key": "q"}]), serde_json::json!("bad"), serde_json::json!({"type": "bearer"}));
        assert_eq!(c.query_params, serde_json::json!([{"key": "q"}]));
        assert_eq!(c.rest_params, serde_json::json!([]));
        assert_eq!(c.auth, serde_json::json!({"type": "bearer"}));
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
