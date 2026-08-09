use std::collections::BTreeMap;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DirectoryError {
    #[error("CFT provenance check failed (abnormal user creation path detected)")]
    ProvenanceCheckFailed,
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait UserDirectory: Send + Sync {
    /// Intercepted validation path; fails with `ProvenanceCheckFailed` for OIDC users.
    async fn names_validated(
        &self,
        ids: &[String],
    ) -> Result<BTreeMap<String, String>, DirectoryError>;

    /// Direct-query bypass, not subject to provenance checks.
    async fn names_direct(
        &self,
        ids: &[String],
    ) -> Result<BTreeMap<String, String>, DirectoryError>;
}
