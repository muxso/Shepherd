//! 验证仓储端口。聚合(标准 + 覆盖链)整体读写。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{NewVerification, Verification};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait VerificationRepository: Send + Sync {
    /// 为某需求版本新建验证(分配 id,写入标准快照)。唯一性(每版本一个)由调用方/DB 兜底。
    async fn create(&self, new: &NewVerification) -> Result<Verification, RepoError>;

    async fn find_by_requirement_version(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Option<Verification>, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<Verification>, RepoError>;

    /// 持久化:更新覆盖链(追加 + satisfied 状态)。标准快照不变。
    async fn save(&self, verification: &Verification) -> Result<(), RepoError>;
}
