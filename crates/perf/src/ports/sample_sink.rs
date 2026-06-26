use async_trait::async_trait;
use thiserror::Error;

use crate::domain::Sample;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SinkError {
    #[error("sample sink error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait SampleSink: Send + Sync {
    async fn write(&self, run_id: &str, samples: &[Sample]) -> Result<String, SinkError>;
}
