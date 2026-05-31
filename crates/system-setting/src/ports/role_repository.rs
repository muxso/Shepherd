//! 角色仓储 + 用户-角色授权端口。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{NewRole, Role};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RoleRepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait RoleRepository: Send + Sync {
    async fn insert(&self, new_role: &NewRole) -> Result<Role, RoleRepoError>;
    async fn get(&self, id: &str) -> Result<Option<Role>, RoleRepoError>;
    async fn count(&self) -> Result<u64, RoleRepoError>;
    async fn list(&self, offset: u64, limit: u32) -> Result<Vec<Role>, RoleRepoError>;
    async fn save(&self, role: &Role) -> Result<(), RoleRepoError>;
    async fn delete(&self, id: &str) -> Result<(), RoleRepoError>;
}

/// 用户 ↔ 角色授权。`effective_permissions` 是登录时算用户有效权限的来源。
#[async_trait]
pub trait UserRoleRepository: Send + Sync {
    async fn grant(&self, user_id: &str, role_id: &str) -> Result<(), RoleRepoError>;
    async fn revoke(&self, user_id: &str, role_id: &str) -> Result<(), RoleRepoError>;
    /// 用户被授予的所有角色的权限并集(原始权限串)。
    async fn effective_permissions(&self, user_id: &str) -> Result<Vec<String>, RoleRepoError>;
}
