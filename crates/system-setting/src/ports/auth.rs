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

    /// 默认 no-op,仅 PG 实现真正重置
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

