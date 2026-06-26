use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EnvironmentError {
    #[error("name must not be empty")]
    EmptyName,
    #[error("project id must not be empty")]
    EmptyProject,
    #[error("base url must start with http:// or https://: {0}")]
    BadBaseUrl(String),
    #[error("header name must not be empty")]
    EmptyHeaderName,
}
