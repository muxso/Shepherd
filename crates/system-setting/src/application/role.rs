//! 角色 CRUD + 用户-角色授权用例。

use std::sync::Arc;

use kernel::page::{Page, PageRequest};
use thiserror::Error;

use crate::domain::{NewRole, Role, RoleError, RoleScope};
use crate::ports::{RoleRepoError, RoleRepository, UserRoleRepository};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RoleCmdError {
    #[error(transparent)]
    Validation(#[from] RoleError),
    #[error("role not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] RoleRepoError),
}

#[derive(Clone)]
pub struct RoleService {
    repo: Arc<dyn RoleRepository>,
}

impl RoleService {
    pub fn new(repo: Arc<dyn RoleRepository>) -> Self {
        Self { repo }
    }

    pub async fn create(
        &self,
        name: &str,
        scope: RoleScope,
        permissions: Vec<String>,
    ) -> Result<Role, RoleCmdError> {
        let new_role = NewRole::new(name, scope, permissions)?;
        Ok(self.repo.insert(&new_role).await?)
    }

    pub async fn list(&self, page: PageRequest) -> Result<Page<Role>, RoleCmdError> {
        let total = self.repo.count().await?;
        let items = self.repo.list(page.offset(), page.page_size()).await?;
        Ok(Page::of(page, total, items))
    }

    pub async fn get(&self, id: &str) -> Result<Role, RoleCmdError> {
        self.repo.get(id).await?.ok_or(RoleCmdError::NotFound)
    }

    /// 改名 + 改权限。
    pub async fn update(
        &self,
        id: &str,
        name: &str,
        permissions: Vec<String>,
    ) -> Result<Role, RoleCmdError> {
        let mut role = self.get(id).await?;
        role.rename(name)?;
        role.set_permissions(permissions)?;
        self.repo.save(&role).await?;
        Ok(role)
    }

    pub async fn delete(&self, id: &str) -> Result<(), RoleCmdError> {
        self.get(id).await?; // 不存在 → NotFound
        self.repo.delete(id).await?;
        Ok(())
    }
}

/// 用户-角色授权。
#[derive(Clone)]
pub struct UserRoleService {
    roles: Arc<dyn RoleRepository>,
    user_roles: Arc<dyn UserRoleRepository>,
}

impl UserRoleService {
    pub fn new(
        roles: Arc<dyn RoleRepository>,
        user_roles: Arc<dyn UserRoleRepository>,
    ) -> Self {
        Self { roles, user_roles }
    }

    /// 授予角色(角色须存在)。
    pub async fn grant(&self, user_id: &str, role_id: &str) -> Result<(), RoleCmdError> {
        if self.roles.get(role_id).await?.is_none() {
            return Err(RoleCmdError::NotFound);
        }
        self.user_roles.grant(user_id, role_id).await?;
        Ok(())
    }

    pub async fn revoke(&self, user_id: &str, role_id: &str) -> Result<(), RoleCmdError> {
        self.user_roles.revoke(user_id, role_id).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{InMemoryRoleRepository, InMemoryUserRoleRepository};

    #[tokio::test]
    async fn role_crud_flow() {
        let repo = Arc::new(InMemoryRoleRepository::new());
        let svc = RoleService::new(repo);
        let r = svc
            .create("auditor", RoleScope::System, vec!["SYSTEM_USER:READ".into()])
            .await
            .expect("create");
        assert_eq!(svc.get(&r.id).await.expect("get").name, "auditor");

        let up = svc
            .update(&r.id, "auditor2", vec!["SYSTEM_USER:READ+ADD".into()])
            .await
            .expect("update");
        assert_eq!(up.name, "auditor2");
        assert_eq!(svc.list(PageRequest::new(1, 10).expect("v")).await.expect("list").total, 1);

        svc.delete(&r.id).await.expect("delete");
        assert_eq!(svc.get(&r.id).await.unwrap_err(), RoleCmdError::NotFound);
    }

    #[tokio::test]
    async fn create_rejects_bad_permission() {
        let svc = RoleService::new(Arc::new(InMemoryRoleRepository::new()));
        assert!(matches!(
            svc.create("r", RoleScope::System, vec!["bogus".into()]).await,
            Err(RoleCmdError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn grant_unknown_role_is_not_found() {
        let roles = Arc::new(InMemoryRoleRepository::new());
        let ur = Arc::new(InMemoryUserRoleRepository::new(roles.clone()));
        let svc = UserRoleService::new(roles, ur);
        assert_eq!(svc.grant("u1", "ghost").await.unwrap_err(), RoleCmdError::NotFound);
    }

    #[tokio::test]
    async fn grant_then_effective_permissions_union() {
        let roles = Arc::new(InMemoryRoleRepository::new());
        let ur = Arc::new(InMemoryUserRoleRepository::new(roles.clone()));
        let role_svc = RoleService::new(roles.clone());
        let a = role_svc.create("a", RoleScope::System, vec!["PROJECT:READ".into()]).await.expect("a");
        let b = role_svc.create("b", RoleScope::System, vec!["BUG:READ+ADD".into()]).await.expect("b");

        let grant = UserRoleService::new(roles, ur.clone());
        grant.grant("u1", &a.id).await.expect("g1");
        grant.grant("u1", &b.id).await.expect("g2");

        let perms = ur.effective_permissions("u1").await.expect("perms");
        assert!(perms.contains(&"PROJECT:READ".to_string()));
        assert!(perms.contains(&"BUG:READ+ADD".to_string()));
    }
}
