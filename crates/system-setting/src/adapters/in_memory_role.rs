use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{NewRole, Role};
use crate::ports::{AuthRepoError, RoleRepoError, RoleRepository, UserRoleQuery, UserRoleRepository};

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
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
        Ok(self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).roles.iter().find(|r| r.id == id).cloned())
    }

    async fn count(&self) -> Result<u64, RoleRepoError> {
        Ok(self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).roles.len() as u64)
    }

    async fn list(&self, offset: u64, limit: u32) -> Result<Vec<Role>, RoleRepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .roles
            .iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn save(&self, role: &Role) -> Result<(), RoleRepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(slot) = st.roles.iter_mut().find(|r| r.id == role.id) {
            *slot = role.clone();
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), RoleRepoError> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).roles.retain(|r| r.id != id);
        Ok(())
    }
}

#[derive(Clone)]
pub struct InMemoryUserRoleRepository {
    roles: Arc<dyn RoleRepository>,
    grants: Arc<Mutex<HashSet<(String, String)>>>,
}

impl InMemoryUserRoleRepository {
    pub fn new(roles: Arc<dyn RoleRepository>) -> Self {
        Self { roles, grants: Arc::new(Mutex::new(HashSet::new())) }
    }
}

#[async_trait]
impl UserRoleRepository for InMemoryUserRoleRepository {
    async fn grant(&self, user_id: &str, role_id: &str) -> Result<(), RoleRepoError> {
        self.grants.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert((user_id.to_string(), role_id.to_string()));
        Ok(())
    }

    async fn revoke(&self, user_id: &str, role_id: &str) -> Result<(), RoleRepoError> {
        self.grants.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&(user_id.to_string(), role_id.to_string()));
        Ok(())
    }

    async fn effective_permissions(&self, user_id: &str) -> Result<Vec<String>, RoleRepoError> {
        let role_ids: Vec<String> = {
            let g = self.grants.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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

#[async_trait]
impl UserRoleQuery for InMemoryUserRoleRepository {
    async fn roles_for(&self, user_ids: &[String]) -> Result<Vec<(String, String)>, AuthRepoError> {
        let grants: Vec<(String, String)> = {
            let g = self.grants.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            g.iter().filter(|(u, _)| user_ids.contains(u)).cloned().collect()
        };
        let mut out = Vec::new();
        for (uid, rid) in grants {
            if let Ok(Some(role)) = self.roles.get(&rid).await {
                out.push((uid, role.name));
            }
        }
        Ok(out)
    }
}
