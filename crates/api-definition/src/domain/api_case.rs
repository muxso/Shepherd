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
    /// 前后置处理器(EXTRACT/WAIT)JSON 数组;默认空。runner 透传,不在此校验形态。
    pub processors: serde_json::Value,
    /// 优先级(P0/P1/P2/P3);默认 P0。对齐 MeterSphere 用例属性。
    pub priority: String,
    /// 用例状态(进行中/已完成/已废弃 等 UI 文本);默认「进行中」。
    pub status: String,
    /// 标签 JSON 数组;默认空。
    pub tags: serde_json::Value,
    /// 请求头 JSON 数组([{key,value}]);默认空。runner 暂不消费,仅随用例往返存储。
    pub headers: serde_json::Value,
    /// Query 参数 JSON 数组([{key,value}]);默认空。
    pub query_params: serde_json::Value,
    /// REST 路径参数 JSON 数组([{key,value}]);默认空。
    pub rest_params: serde_json::Value,
    /// 认证配置 JSON 对象({type,token});默认空对象。
    pub auth: serde_json::Value,
}

/// 用例默认优先级 / 状态(与前端 select 缺省保持一致)。
const DEFAULT_PRIORITY: &str = "P0";
const DEFAULT_STATUS: &str = "进行中";

impl NewApiCase {
    /// 校验:name/url 非空(trim);method 在 HTTP 方法白名单内并规整大写;
    /// assertions 必须是 JSON 数组。processors 默认空数组(见 [`with_processors`](Self::with_processors))。
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

    /// 附加前后置处理器(非数组则回落空数组,保持存储形态稳定)。
    pub fn with_processors(mut self, processors: serde_json::Value) -> Self {
        self.processors =
            if processors.is_array() { processors } else { serde_json::Value::Array(Vec::new()) };
        self
    }

    /// 附加用例元信息:优先级 / 状态 / 标签(对齐 MeterSphere)。
    /// 空白优先级/状态回落默认值;tags 非数组回落空数组。
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

    /// 附加请求头(非数组则回落空数组)。
    pub fn with_headers(mut self, headers: serde_json::Value) -> Self {
        self.headers =
            if headers.is_array() { headers } else { serde_json::Value::Array(Vec::new()) };
        self
    }

    /// 附加请求规格:Query / REST 路径参数(数组)/ 认证(对象)。非期望形态各自回落默认。
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
        // 空白优先级/状态回落默认;tags 透传数组。
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
