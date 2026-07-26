//! DB-backed OIDC provider configuration.
//!
//! Kept free of `serde` derives on purpose: the `domain` layer is always
//! compiled, while `serde`/`serde_json` are only pulled in by the `http`
//! feature. Serialization is the adapter layer's job (DTOs in `adapters::http`).

/// A single configured external identity provider, persisted by the
/// `OidcProviderRepository`. `provider_key` is the stable lookup key and must
/// match the `ExternalIdentityProvider::key()` that the corresponding adapter
/// reports (e.g. "feishu", "wecom", "lark", "dingtalk", "slack").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcProvider {
    pub provider_key: String,
    pub app_id: String,
    pub app_secret: String,
    pub redirect: String,
    pub default_permissions: Vec<String>,
    pub enabled: bool,
    /// Optional vendor base URL override (e.g. choose the Lark domain, or point
    /// at a mock server in tests). `None` uses the adapter's built-in default.
    pub base_url: Option<String>,
}

/// Errors surfaced by the OIDC provider repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OidcRepoError {
    Backend(String),
    NotFound,
}

impl std::fmt::Display for OidcRepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OidcRepoError::Backend(m) => write!(f, "oidc provider storage error: {m}"),
            OidcRepoError::NotFound => write!(f, "oidc provider not found"),
        }
    }
}

impl std::error::Error for OidcRepoError {}
