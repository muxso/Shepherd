//! 未知用户与密码错误返回同一 `InvalidCredentials`,防账号枚举。

use std::sync::Arc;

use kernel::permission::PermissionSet;

use crate::domain::AuthError;
use crate::ports::{
    CredentialRepository, DirectoryAuthenticator, PasswordHasher, SessionStore, UserCredential,
    UserRoleRepository,
};

#[derive(Clone)]
pub struct LoginUseCase {
    creds: Arc<dyn CredentialRepository>,
    hasher: Arc<dyn PasswordHasher>,
    sessions: Arc<dyn SessionStore>,
    user_roles: Arc<dyn UserRoleRepository>,
    directory: Option<Arc<dyn DirectoryAuthenticator>>,
    session_ttl_secs: i64,
}

impl LoginUseCase {
    pub fn new(
        creds: Arc<dyn CredentialRepository>,
        hasher: Arc<dyn PasswordHasher>,
        sessions: Arc<dyn SessionStore>,
        user_roles: Arc<dyn UserRoleRepository>,
    ) -> Self {
        Self { creds, hasher, sessions, user_roles, directory: None, session_ttl_secs: 8 * 3600 }
    }

    pub fn with_ttl_secs(mut self, secs: i64) -> Self {
        self.session_ttl_secs = secs;
        self
    }

    /// 配置外部目录(LDAP 等)作为本地口令之外的认证回退。
    pub fn with_directory(mut self, dir: Arc<dyn DirectoryAuthenticator>) -> Self {
        self.directory = Some(dir);
        self
    }

    /// 本地口令优先;不符且配置了外部目录时回退到目录绑定。
    /// 未知用户一律 `InvalidCredentials`,既不枚举也不自动开户。
    async fn authenticated(
        &self,
        username: &str,
        password: &str,
        cred: &UserCredential,
    ) -> Result<bool, AuthError> {
        if self.hasher.verify(password, &cred.password_hash) {
            return Ok(true);
        }
        match &self.directory {
            Some(dir) => dir
                .authenticate(username, password)
                .await
                .map_err(|e| AuthError::Backend(e.to_string())),
            None => Ok(false),
        }
    }

    pub async fn execute(&self, username: &str, password: &str) -> Result<String, AuthError> {
        let cred = self
            .creds
            .find_by_username(username)
            .await
            .map_err(|e| AuthError::Backend(e.to_string()))?
            .ok_or(AuthError::InvalidCredentials)?;

        if !self.authenticated(username, password, &cred).await? {
            return Err(AuthError::InvalidCredentials);
        }

        // 有效权限 = 凭证自带 ∪ 用户所有角色权限的并集
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
    use crate::ports::AuthRepoError;

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
            "secret",
            ["SYSTEM_USER:READ+ADD"],
        )
    }

    #[tokio::test]
    async fn correct_credentials_yield_token() {
        let (uc, sessions) = uc(repo_with_admin());
        let token = uc.execute("admin", "secret").await.expect("login ok");
        assert!(!token.is_empty());
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
        assert_eq!(uc.execute("ghost", "secret").await, Err(AuthError::InvalidCredentials));
    }

    /// 桩目录:按构造期望回应(成功/拒绝/后端故障)。
    struct StubDirectory(Result<bool, AuthRepoError>);
    #[async_trait::async_trait]
    impl crate::ports::DirectoryAuthenticator for StubDirectory {
        async fn authenticate(&self, _u: &str, _p: &str) -> Result<bool, AuthRepoError> {
            self.0.clone()
        }
    }

    fn uc_with_dir(
        repo: InMemoryCredentialRepository,
        dir: StubDirectory,
    ) -> (LoginUseCase, Arc<InMemorySessionStore>) {
        let (uc, sessions) = uc(repo);
        (uc.with_directory(Arc::new(dir)), sessions)
    }

    #[tokio::test]
    async fn directory_authenticates_when_local_password_wrong() {
        // 本地口令不符,但目录绑定成功 → 外部认证 + 本地授权,发令牌。
        let (uc, sessions) = uc_with_dir(repo_with_admin(), StubDirectory(Ok(true)));
        let token = uc.execute("admin", "ldap-pw").await.expect("dir login ok");
        let session = sessions.get(&token).await.expect("ok").expect("session");
        assert!(session.permissions.allows("SYSTEM_USER", "ADD")); // 权限仍来自本地角色
    }

    #[tokio::test]
    async fn local_password_short_circuits_directory() {
        // 本地口令正确 → 即使目录会故障也不应被调用,登录照常成功。
        let (uc, _) = uc_with_dir(
            repo_with_admin(),
            StubDirectory(Err(AuthRepoError::Backend("must not be called".into()))),
        );
        assert!(uc.execute("admin", "secret").await.is_ok());
    }

    #[tokio::test]
    async fn directory_rejection_is_invalid_credentials() {
        let (uc, _) = uc_with_dir(repo_with_admin(), StubDirectory(Ok(false)));
        assert_eq!(uc.execute("admin", "nope").await, Err(AuthError::InvalidCredentials));
    }

    #[tokio::test]
    async fn directory_backend_error_is_distinct_from_bad_password() {
        let (uc, _) = uc_with_dir(
            repo_with_admin(),
            StubDirectory(Err(AuthRepoError::Backend("ldap down".into()))),
        );
        assert!(matches!(uc.execute("admin", "x").await, Err(AuthError::Backend(_))));
    }

    #[tokio::test]
    async fn unknown_user_not_provisioned_via_directory() {
        // 目录会接受任何人,但本地无此用户 → 不开户、不枚举。
        let (uc, _) = uc_with_dir(repo_with_admin(), StubDirectory(Ok(true)));
        assert_eq!(uc.execute("ghost", "any").await, Err(AuthError::InvalidCredentials));
    }
}
