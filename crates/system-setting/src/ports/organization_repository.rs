//! 组织仓储端口。`*_active` 一律忽略软删除(语义固化进契约)。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{NewOrganization, Organization};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrgRepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait OrgRepository: Send + Sync {
    async fn insert(&self, new_org: &NewOrganization) -> Result<Organization, OrgRepoError>;
    /// 未删除组织里按名查找(唯一性判定)。
    async fn find_active_by_name(&self, name: &str) -> Result<Option<Organization>, OrgRepoError>;
    async fn get(&self, id: &str) -> Result<Option<Organization>, OrgRepoError>;
    async fn count_active(&self) -> Result<u64, OrgRepoError>;
    async fn list_active(&self, offset: u64, limit: u32) -> Result<Vec<Organization>, OrgRepoError>;
    /// 保存(改名/启停/软删除标记)。
    async fn save(&self, org: &Organization) -> Result<(), OrgRepoError>;
}
