use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApiDefinitionError {
    #[error("name must not be empty")]
    EmptyName,
    #[error("project id must not be empty")]
    EmptyProject,
    #[error("url must not be empty")]
    EmptyUrl,
    #[error("unknown http method: {0}")]
    UnknownMethod(String),
    #[error("assertions must be a json array")]
    BadAssertions,
    #[error("unknown protocol: {0}")]
    BadProtocol(String),
    #[error("unknown status: {0}")]
    BadStatus(String),
    #[error("response status out of range: {0}")]
    BadResponseStatus(i32),
    #[error("unrecognized import document: {0}")]
    BadImport(String),
    #[error("config must be a json object")]
    BadConfig,
}

pub(crate) fn normalize_http_method(method: &str) -> Result<String, ApiDefinitionError> {
    let upper = method.trim().to_uppercase();
    match upper.as_str() {
        "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS" => Ok(upper),
        _ => Err(ApiDefinitionError::UnknownMethod(method.to_string())),
    }
}
