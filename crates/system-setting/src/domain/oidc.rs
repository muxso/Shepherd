use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalIdentity {
    pub provider: String,
    pub open_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OidcError {
    #[error("unknown identity provider: {0}")]
    UnknownProvider(String),
    #[error("identity exchange failed: {0}")]
    Exchange(String),
    #[error("backend error: {0}")]
    Backend(String),
}
