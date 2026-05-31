//! 用例:登录。校验凭证 → 建会话 → 返回令牌。注入假实现即可测,无需密码学库。
//!
//! 安全要点:未知用户与密码错误返回**同一** `InvalidCredentials`(防账号枚举)。

use std::sync::Arc;

use kernel::permission::PermissionSet;

use crate::domain::AuthError;
use crate::ports::{CredentialRepository, PasswordHasher, SessionStore, UserRoleRepository};

#[derive(Clone)]
pub struct LoginUseCase {
    creds: Arc<dyn CredentialRepository>,
    hasher: Arc<dyn PasswordHasher>,
    sessions: Arc<dyn SessionStore>,
    user_roles: Arc<dyn UserRoleRepository>,
    session_ttl_secs: i64,
}

impl LoginUseCase {
    pub fn new(
        creds: Arc<dyn CredentialRepository>,
        hasher: Arc<dyn PasswordHasher>,
        sessions: Arc<dyn SessionStore>,
        user_roles: Arc<dyn UserRoleRepository>,
    ) -> Self {
        Self { creds, hasher, sessions, user_roles, session_ttl_secs: 8 * 3600 }
    }

    /// 设置会话有效期(秒)。默认 8 小时。
    pub fn with_ttl_secs(mut self, secs: i64) -> Self {
        self.session_ttl_secs = secs;
        self
    }

    /// 成功返回令牌。
    pub async fn execute(&self, username: &str, password: &str) -> Result<String, AuthError> {
        let cred = self
            .creds
            .find_by_username(username)
            .await
            .map_err(|e| AuthError::Backend(e.to_string()))?
            .ok_or(AuthError::InvalidCredentials)?;

        if !self.hasher.verify(password, &cred.password_hash) {
            return Err(AuthError::InvalidCredentials);
        }

        // 有效权限 = 凭证自带 ∪ 用户所有角色的权限并集(RBAC 闭环)
        let mut raw = cred.permissions.clone();
        raw.extend(
            self.user_roles
                .effective_permissions(&cred.user_id)
                .await
                .map_err(|e| AuthError::Backend(e.to_string()))?,
        );
        let permissions = PermissionSet::from_raw(&raw)
            .map_err(|_| AuthError::Backend("invalid permission config".into()))?;

        self.sessions
            .create(&cred.user_id, permissions, self.session_ttl_secs)
            .await
            .map_err(|e| AuthError::Backend(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{
        InMemoryCredentialRepository, InMemoryRoleRepository, InMemorySessionStore,
        InMemoryUserRoleRepository, PlainPasswordHasher,
    };

    fn uc(repo: InMemoryCredentialRepository) -> (LoginUseCase, Arc<InMemorySessionStore>) {
        let sessions = Arc::new(InMemorySessionStore::new());
        let roles = Arc::new(InMemoryRoleRepository::new());
        let user_roles = Arc::new(InMemoryUserRoleRepository::new(roles));
        let uc = LoginUseCase::new(
            Arc::new(repo),
            Arc::new(PlainPasswordHasher),
            sessions.clone(),
            user_roles,
        );
        (uc, sessions)
    }

    fn repo_with_admin() -> InMemoryCredentialRepository {
        InMemoryCredentialRepository::new().with_user(
            "admin",
            "u-admin",
            "secret", // PlainPasswordHasher:hash == 明文
            ["SYSTEM_USER:READ+ADD"],
        )
    }

    #[tokio::test]
    async fn correct_credentials_yield_token() {
        let (uc, sessions) = uc(repo_with_admin());
        let token = uc.execute("admin", "secret").await.expect("login ok");
        assert!(!token.is_empty());
        // 会话可按令牌取回,且带权限
        let session = sessions.get(&token).await.expect("ok").expect("session");
        assert!(session.permissions.allows("SYSTEM_USER", "ADD"));
    }

    #[tokio::test]
    async fn wrong_password_is_invalid_credentials() {
        let (uc, _) = uc(repo_with_admin());
        assert_eq!(uc.execute("admin", "nope").await, Err(AuthError::InvalidCredentials));
    }

    #[tokio::test]
    async fn unknown_user_is_same_error_no_enumeration() {
        let (uc, _) = uc(repo_with_admin());
        // 与密码错误返回同一错误,无法据此判断用户是否存在
        assert_eq!(uc.execute("ghost", "secret").await, Err(AuthError::InvalidCredentials));
    }
}
