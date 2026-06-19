//! 环境上下文的领域校验错误。

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EnvironmentError {
    /// 名称为空(已 trim)。
    #[error("name must not be empty")]
    EmptyName,
    /// 项目 ID 为空。
    #[error("project id must not be empty")]
    EmptyProject,
    /// base_url 非空但形态非法(必须以 http:// 或 https:// 开头)。
    #[error("base url must start with http:// or https://: {0}")]
    BadBaseUrl(String),
    /// 请求头名为空。
    #[error("header name must not be empty")]
    EmptyHeaderName,
}
