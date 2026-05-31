//! 用户目录端口。两条查询路径还原 OIDC quirk:校验路径(受 CFT 拦截)与直查旁路。

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
    /// 受 CFT/Liber SDK 拦截的查询路径。对 OIDC/外部用户会 `ProvenanceCheckFailed`。
    async fn names_validated(
        &self,
        ids: &[String],
    ) -> Result<BTreeMap<String, String>, DirectoryError>;

    /// 直查用户表的旁路:provenance 无关,解析所有用户。
    async fn names_direct(
        &self,
        ids: &[String],
    ) -> Result<BTreeMap<String, String>, DirectoryError>;
}
