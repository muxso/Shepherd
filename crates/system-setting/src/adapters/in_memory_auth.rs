use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use kernel::permission::PermissionSet;

use crate::domain::Session;
use crate::ports::{
    AuthRepoError, CredentialRepository, PasswordHasher, SessionStore, UserCredential,
};

// Mutex:自助改密要求在共享后仍可写(axum state 克隆间共享同一份)。
#[derive(Clone, Default)]
pub struct InMemoryCredentialRepository {
    users: Arc<Mutex<HashMap<String, UserCredential>>>,
}

impl InMemoryCredentialRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_user<I, S>(
        self,
        username: &str,
        user_id: &str,
        password_hash: &str,
        perms: I,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.users.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(
            username.to_string(),
            UserCredential {
                user_id: user_id.to_string(),
                password_hash: password_hash.to_string(),
                permissions: perms.into_iter().map(Into::into).collect(),
            },
        );
        self
    }
}

#[async_trait]
impl CredentialRepository for InMemoryCredentialRepository {
    async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserCredential>, AuthRepoError> {
        let users = self.users.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(users.get(username).cloned())
    }

    async fn find_by_user_id(
        &self,
        user_id: &str,
    ) -> Result<Option<UserCredential>, AuthRepoError> {
        let users = self.users.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(users.values().find(|c| c.user_id == user_id).cloned())
    }

    async fn update_password(
        &self,
        user_id: &str,
        password_hash: &str,
    ) -> Result<bool, AuthRepoError> {
        let mut users = self.users.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match users.values_mut().find(|c| c.user_id == user_id) {
            Some(c) => {
                c.password_hash = password_hash.to_string();
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

#[derive(Default)]
struct SessionState {
    sessions: HashMap<String, Session>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemorySessionStore {
    state: Arc<Mutex<SessionState>>,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_expired(&self, token: &str, user_id: &str) {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).sessions.insert(
            token.to_string(),
            Session {
                token: token.to_string(),
                user_id: user_id.to_string(),
                permissions: PermissionSet::default(),
                expires_at_ms: 0,
            },
        );
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create(
        &self,
        user_id: &str,
        permissions: PermissionSet,
        ttl_secs: i64,
    ) -> Result<String, AuthRepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        st.seq += 1;
        let token = format!("sess-{}", st.seq);
        st.sessions.insert(
            token.clone(),
            Session {
                token: token.clone(),
                user_id: user_id.to_string(),
                permissions,
                expires_at_ms: now_ms() + ttl_secs * 1000,
            },
        );
        Ok(token)
    }

    async fn get(&self, token: &str) -> Result<Option<Session>, AuthRepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(match st.sessions.get(token) {
            Some(s) if s.expires_at_ms > now_ms() => Some(s.clone()),
            _ => None,
        })
    }

    async fn revoke(&self, token: &str) -> Result<(), AuthRepoError> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).sessions.remove(token);
        Ok(())
    }
}

/// 仅测试/本地:hash 原样返回明文;生产用 `Argon2PasswordHasher`(feature=auth)。
#[derive(Clone, Default)]
pub struct PlainPasswordHasher;

impl PasswordHasher for PlainPasswordHasher {
    fn hash(&self, plain: &str) -> String {
        plain.to_string()
    }
    fn verify(&self, plain: &str, hash: &str) -> bool {
        plain == hash
    }
}
