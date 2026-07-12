//! Unknown user and wrong password return the same `InvalidCredentials`, preventing
//! account enumeration.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use kernel::permission::PermissionSet;

use crate::domain::AuthError;
use crate::ports::{
    CredentialRepository, DirectoryAuthenticator, PasswordHasher, SessionStore, UserCredential,
    UserRoleRepository,
};

/// Lockout triggers after this many consecutive failures for the same username.
pub const MAX_LOGIN_FAILURES: u32 = 5;
/// Lockout duration; the counter resets once it elapses.
pub const LOCKOUT_WINDOW: Duration = Duration::from_secs(15 * 60);

/// Tracks consecutive failures per username (in-memory, cleared on process restart).
/// Unknown usernames are counted too, so lockout behaves identically for existing and
/// non-existing accounts and leaks no enumeration signal.
#[derive(Default)]
struct FailureTracker {
    entries: Mutex<HashMap<String, (u32, Instant)>>,
}

impl FailureTracker {
    /// Whether the username is in the lockout window; clears the counter if it has expired.
    fn is_locked(&self, username: &str, now: Instant) -> bool {
        let mut g = self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match g.get(username) {
            Some(&(fails, last)) if fails >= MAX_LOGIN_FAILURES => {
                if now.duration_since(last) < LOCKOUT_WINDOW {
                    true
                } else {
                    g.remove(username);
                    false
                }
            }
            _ => false,
        }
    }

    fn record_failure(&self, username: &str, now: Instant) {
        let mut g = self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let e = g.entry(username.to_string()).or_insert((0, now));
        e.0 += 1;
        e.1 = now;
    }

    fn reset(&self, username: &str) {
        self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(username);
    }
}

#[derive(Clone)]
pub struct LoginUseCase {
    creds: Arc<dyn CredentialRepository>,
    hasher: Arc<dyn PasswordHasher>,
    sessions: Arc<dyn SessionStore>,
    user_roles: Arc<dyn UserRoleRepository>,
    directory: Option<Arc<dyn DirectoryAuthenticator>>,
    session_ttl_secs: i64,
    // Arc: axum clones state per request; the counter must be shared across clones.
    failures: Arc<FailureTracker>,
    clock: Arc<dyn Fn() -> Instant + Send + Sync>,
}

impl LoginUseCase {
    pub fn new(
        creds: Arc<dyn CredentialRepository>,
        hasher: Arc<dyn PasswordHasher>,
        sessions: Arc<dyn SessionStore>,
        user_roles: Arc<dyn UserRoleRepository>,
    ) -> Self {
        Self {
            creds,
            hasher,
            sessions,
            user_roles,
            directory: None,
            session_ttl_secs: 8 * 3600,
            failures: Arc::new(FailureTracker::default()),
            clock: Arc::new(Instant::now),
        }
    }

    pub fn with_ttl_secs(mut self, secs: i64) -> Self {
        self.session_ttl_secs = secs;
        self
    }

    /// Test hook: inject an adjustable clock so tests need not wait out the lockout window.
    #[cfg(test)]
    fn with_clock(mut self, clock: Arc<dyn Fn() -> Instant + Send + Sync>) -> Self {
        self.clock = clock;
        self
    }

    /// Configure an external directory (LDAP etc.) as an auth fallback after local password.
    pub fn with_directory(mut self, dir: Arc<dyn DirectoryAuthenticator>) -> Self {
        self.directory = Some(dir);
        self
    }

    /// Local password first; on mismatch, falls back to a directory bind if configured.
    /// Unknown users always get `InvalidCredentials` — no enumeration, no auto-provisioning.
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
        let now = (self.clock)();
        // Reject before password verification during lockout: even the correct password fails.
        if self.failures.is_locked(username, now) {
            return Err(AuthError::LockedOut);
        }
        let result = self.attempt(username, password).await;
        match &result {
            Ok(_) => self.failures.reset(username),
            Err(AuthError::InvalidCredentials) => self.failures.record_failure(username, now),
            // Backend failures don't count against the user.
            Err(_) => {}
        }
        result
    }

    async fn attempt(&self, username: &str, password: &str) -> Result<String, AuthError> {
        let cred = self
            .creds
            .find_by_username(username)
            .await
            .map_err(|e| AuthError::Backend(e.to_string()))?
            .ok_or(AuthError::InvalidCredentials)?;

        if !self.authenticated(username, password, &cred).await? {
            return Err(AuthError::InvalidCredentials);
        }

        // Effective permissions = credential's own ∪ union of all the user's role permissions.
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

    /// Stub directory: replies as constructed (success/rejection/backend failure).
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
        // Local password wrong but directory bind succeeds → external auth + local
        // authorization, token issued.
        let (uc, sessions) = uc_with_dir(repo_with_admin(), StubDirectory(Ok(true)));
        let token = uc.execute("admin", "ldap-pw").await.expect("dir login ok");
        let session = sessions.get(&token).await.expect("ok").expect("session");
        assert!(session.permissions.allows("SYSTEM_USER", "ADD")); // permissions still from local roles
    }

    #[tokio::test]
    async fn local_password_short_circuits_directory() {
        // Correct local password → directory must not be called even if it would fail;
        // login succeeds as usual.
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
        // Directory accepts anyone, but the user does not exist locally → no provisioning,
        // no enumeration.
        let (uc, _) = uc_with_dir(repo_with_admin(), StubDirectory(Ok(true)));
        assert_eq!(uc.execute("ghost", "any").await, Err(AuthError::InvalidCredentials));
    }

    /// Adjustable clock: tests need not actually wait out the lockout window.
    fn mock_clock() -> (Arc<Mutex<Instant>>, Arc<dyn Fn() -> Instant + Send + Sync>) {
        let now = Arc::new(Mutex::new(Instant::now()));
        let handle = now.clone();
        let clock: Arc<dyn Fn() -> Instant + Send + Sync> =
            Arc::new(move || *handle.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        (now, clock)
    }

    async fn fail_n(uc: &LoginUseCase, user: &str, n: u32) {
        for _ in 0..n {
            assert_eq!(uc.execute(user, "wrong").await, Err(AuthError::InvalidCredentials));
        }
    }

    #[tokio::test]
    async fn lockout_after_max_failures() {
        let (uc, _) = uc(repo_with_admin());
        fail_n(&uc, "admin", MAX_LOGIN_FAILURES).await;
        // From the 6th attempt on, the error is lockout, not bad password.
        assert_eq!(uc.execute("admin", "wrong").await, Err(AuthError::LockedOut));
    }

    #[tokio::test]
    async fn correct_password_rejected_during_lockout() {
        let (uc, _) = uc(repo_with_admin());
        fail_n(&uc, "admin", MAX_LOGIN_FAILURES).await;
        assert_eq!(uc.execute("admin", "secret").await, Err(AuthError::LockedOut));
    }

    #[tokio::test]
    async fn success_resets_failure_counter() {
        let (uc, _) = uc(repo_with_admin());
        fail_n(&uc, "admin", MAX_LOGIN_FAILURES - 1).await;
        assert!(uc.execute("admin", "secret").await.is_ok());
        // Counter was reset: one more failure is still a bad password, not cumulative lockout.
        fail_n(&uc, "admin", 1).await;
        assert!(uc.execute("admin", "secret").await.is_ok());
    }

    #[tokio::test]
    async fn lockout_expires_after_window() {
        let (now, clock) = mock_clock();
        let (uc, _) = uc(repo_with_admin());
        let uc = uc.with_clock(clock);
        fail_n(&uc, "admin", MAX_LOGIN_FAILURES).await;
        assert_eq!(uc.execute("admin", "secret").await, Err(AuthError::LockedOut));
        // Advance past the lockout window → counter cleared, correct password works again.
        *now.lock().unwrap_or_else(std::sync::PoisonError::into_inner) +=
            LOCKOUT_WINDOW + Duration::from_secs(1);
        assert!(uc.execute("admin", "secret").await.is_ok());
    }

    #[tokio::test]
    async fn lockout_is_per_username() {
        let (uc, _) = uc(repo_with_admin().with_user("bob", "u-bob", "pw", ["SYSTEM_USER:READ"]));
        fail_n(&uc, "admin", MAX_LOGIN_FAILURES).await;
        assert_eq!(uc.execute("admin", "secret").await, Err(AuthError::LockedOut));
        assert!(uc.execute("bob", "pw").await.is_ok());
    }
}
