use async_trait::async_trait;

pub use webauth::{AuthRepoError, SessionStore};

#[derive(Debug, Clone)]
pub struct UserCredential {
    pub user_id: String,
    pub password_hash: String,
    pub permissions: Vec<String>,
}

#[async_trait]
pub trait CredentialRepository: Send + Sync {
    async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserCredential>, AuthRepoError>;

    /// Sessions only carry user_id; self-service password change looks up credentials by it.
    /// Defaults to None: the credential is not stored (e.g. env-injected built-in account),
    /// so the store cannot change it.
    async fn find_by_user_id(
        &self,
        _user_id: &str,
    ) -> Result<Option<UserCredential>, AuthRepoError> {
        Ok(None)
    }

    /// Update the password hash; `false` means the store has no credential for this user.
    async fn update_password(
        &self,
        _user_id: &str,
        _password_hash: &str,
    ) -> Result<bool, AuthRepoError> {
        Ok(false)
    }

    /// Default no-op; only the PG implementation actually resets.
    async fn reset_password(
        &self,
        _user_id: &str,
        _username: &str,
        _password_hash: &str,
    ) -> Result<(), AuthRepoError> {
        Ok(())
    }
}

#[async_trait]
pub trait UserRoleQuery: Send + Sync {
    async fn roles_for(&self, user_ids: &[String]) -> Result<Vec<(String, String)>, AuthRepoError>;
}

pub trait PasswordHasher: Send + Sync {
    fn hash(&self, plain: &str) -> String;
    fn verify(&self, plain: &str, hash: &str) -> bool;
}

/// External directory authentication (LDAP etc.): only checks whether username + password
/// can bind. Authorization still comes from local roles/permissions (**local authorization +
/// external authentication**); the user must already exist locally.
/// `Ok(true)` bind succeeded; `Ok(false)` credentials rejected; `Err` directory backend
/// unavailable (distinct from a wrong password).
#[async_trait]
pub trait DirectoryAuthenticator: Send + Sync {
    async fn authenticate(&self, username: &str, password: &str) -> Result<bool, AuthRepoError>;
}
