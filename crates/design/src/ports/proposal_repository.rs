use async_trait::async_trait;
use thiserror::Error;

use crate::domain::Proposal;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait ProposalRepository: Send + Sync {
    async fn create(&self, requirement_id: &str, title: &str) -> Result<Proposal, RepoError>;
    async fn get(&self, id: &str) -> Result<Option<Proposal>, RepoError>;
    async fn save(&self, proposal: &Proposal) -> Result<(), RepoError>;
    async fn list_by_requirement(&self, requirement_id: &str) -> Result<Vec<Proposal>, RepoError>;
}
