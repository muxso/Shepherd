//! 接口定义上下文的领域校验错误。

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApiDefinitionError {
    /// 名称为空(已 trim)。
    #[error("name must not be empty")]
    EmptyName,
    /// 项目 ID 为空。
    #[error("project id must not be empty")]
    EmptyProject,
    /// URL 为空(用例)。
    #[error("url must not be empty")]
    EmptyUrl,
    /// HTTP 方法不在白名单内。
    #[error("unknown http method: {0}")]
    UnknownMethod(String),
    /// 断言不是 JSON 数组。
    #[error("assertions must be a json array")]
    BadAssertions,
    /// 未知协议。
    #[error("unknown protocol: {0}")]
    BadProtocol(String),
    /// 未知状态。
    #[error("unknown status: {0}")]
    BadStatus(String),
    /// Mock 响应状态码越界(应在 100..=599)。
    #[error("response status out of range: {0}")]
    BadResponseStatus(i32),
}

/// HTTP 方法白名单。把任意大小写规整成大写后校验,返回规整后的方法名。
pub(crate) fn normalize_http_method(method: &str) -> Result<String, ApiDefinitionError> {
    let upper = method.trim().to_uppercase();
    match upper.as_str() {
        "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS" => Ok(upper),
        _ => Err(ApiDefinitionError::UnknownMethod(method.to_string())),
    }
}
