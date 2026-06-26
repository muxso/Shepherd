use async_trait::async_trait;
use thiserror::Error;

use crate::domain::Proposal;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TriggerError {
    #[error("breakdown trigger error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait BreakdownTrigger: Send + Sync {
    async fn on_design_approved(&self, proposal: &Proposal) -> Result<(), TriggerError>;
}
