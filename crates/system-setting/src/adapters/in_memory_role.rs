//! 内存版角色仓储 + 用户-角色授权(test double 兼本地)。

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{NewRole, Role};
use crate::ports::{RoleRepoError, RoleRepository, UserRoleRepository};

#[derive(Default)]
struct RoleState {
    roles: Vec<Role>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryRoleRepository {
    state: Arc<Mutex<RoleState>>,
}

impl InMemoryRoleRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl RoleRepository for InMemoryRoleRepository {
    async fn insert(&self, new_role: &NewRole) -> Result<Role, RoleRepoError> {
        let mut st = self.state.lock().expect("lock");
        st.seq += 1;
        let role = Role {
            id: format!("role-{}", st.seq),
            name: new_role.name.clone(),
            scope: new_role.scope,
            permissions: new_role.permissions.clone(),
        };
        st.roles.push(role.clone());
        Ok(role)
    }

    async fn get(&self, id: &str) -> Result<Option<Role>, RoleRepoError> {
        Ok(self.state.lock().expect("lock").roles.iter().find(|r| r.id == id).cloned())
    }

    async fn count(&self) -> Result<u64, RoleRepoError> {
        Ok(self.state.lock().expect("lock").roles.len() as u64)
    }

    async fn list(&self, offset: u64, limit: u32) -> Result<Vec<Role>, RoleRepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .roles
            .iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn save(&self, role: &Role) -> Result<(), RoleRepoError> {
        let mut st = self.state.lock().expect("lock");
        if let Some(slot) = st.roles.iter_mut().find(|r| r.id == role.id) {
            *slot = role.clone();
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), RoleRepoError> {
        self.state.lock().expect("lock").roles.retain(|r| r.id != id);
        Ok(())
    }
}

/// 内存用户-角色授权。持有角色仓储以计算有效权限并集。
#[derive(Clone)]
pub struct InMemoryUserRoleRepository {
    roles: Arc<dyn RoleRepository>,
    grants: Arc<Mutex<HashSet<(String, String)>>>, // (user_id, role_id)
}

impl InMemoryUserRoleRepository {
    pub fn new(roles: Arc<dyn RoleRepository>) -> Self {
        Self { roles, grants: Arc::new(Mutex::new(HashSet::new())) }
    }
}

#[async_trait]
impl UserRoleRepository for InMemoryUserRoleRepository {
    async fn grant(&self, user_id: &str, role_id: &str) -> Result<(), RoleRepoError> {
        self.grants.lock().expect("lock").insert((user_id.to_string(), role_id.to_string()));
        Ok(())
    }

    async fn revoke(&self, user_id: &str, role_id: &str) -> Result<(), RoleRepoError> {
        self.grants.lock().expect("lock").remove(&(user_id.to_string(), role_id.to_string()));
        Ok(())
    }

    async fn effective_permissions(&self, user_id: &str) -> Result<Vec<String>, RoleRepoError> {
        let role_ids: Vec<String> = {
            let g = self.grants.lock().expect("lock");
            g.iter().filter(|(u, _)| u == user_id).map(|(_, r)| r.clone()).collect()
        };
        let mut set: HashSet<String> = HashSet::new();
        for rid in role_ids {
            if let Some(role) = self.roles.get(&rid).await? {
                set.extend(role.permissions);
            }
        }
        Ok(set.into_iter().collect())
    }
}
