use std::collections::BTreeMap;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DirectoryError {
    #[error("CFT provenance check failed (检测到用户创建途径异常)")]
    ProvenanceCheckFailed,
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait UserDirectory: Send + Sync {
    /// 受拦截的校验路径,对 OIDC 用户会 `ProvenanceCheckFailed`
    async fn names_validated(
        &self,
        ids: &[String],
    ) -> Result<BTreeMap<String, String>, DirectoryError>;

    /// 直查旁路,不受 provenance 限制
    async fn names_direct(
        &self,
        ids: &[String],
    ) -> Result<BTreeMap<String, String>, DirectoryError>;
}
