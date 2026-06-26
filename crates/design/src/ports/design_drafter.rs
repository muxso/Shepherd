use async_trait::async_trait;
use thiserror::Error;

use crate::domain::Proposal;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DraftError {
    #[error("drafter backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait DesignDrafter: Send + Sync {
    async fn request_draft(&self, proposal: &Proposal) -> Result<(), DraftError>;
}
