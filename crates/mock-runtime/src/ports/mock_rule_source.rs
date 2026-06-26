use async_trait::async_trait;
use thiserror::Error;

use crate::domain::MockRule;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SourceError {
    #[error("rule source error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait MockRuleSource: Send + Sync {
    async fn active_rules(&self) -> Result<Vec<MockRule>, SourceError>;
}
