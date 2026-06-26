use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{NewVerification, Verification};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait VerificationRepository: Send + Sync {
    async fn create(&self, new: &NewVerification) -> Result<Verification, RepoError>;

    async fn find_by_requirement_version(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Option<Verification>, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<Verification>, RepoError>;

    async fn save(&self, verification: &Verification) -> Result<(), RepoError>;
}
