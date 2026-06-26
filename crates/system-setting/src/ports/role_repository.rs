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

#[async_trait]
pub trait UserRoleRepository: Send + Sync {
    async fn grant(&self, user_id: &str, role_id: &str) -> Result<(), RoleRepoError>;
    async fn revoke(&self, user_id: &str, role_id: &str) -> Result<(), RoleRepoError>;
    async fn effective_permissions(&self, user_id: &str) -> Result<Vec<String>, RoleRepoError>;
}
