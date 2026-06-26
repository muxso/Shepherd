use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{Email, NewUser, User};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_email(&self, email: &Email) -> Result<Option<User>, RepoError>;
    async fn insert(&self, new_user: &NewUser) -> Result<User, RepoError>;
    async fn get(&self, id: &str) -> Result<Option<User>, RepoError>;
    async fn count_active(&self) -> Result<u64, RepoError>;
    async fn list_active(&self, offset: u64, limit: u32) -> Result<Vec<User>, RepoError>;
    async fn save(&self, user: &User) -> Result<(), RepoError>;
}
