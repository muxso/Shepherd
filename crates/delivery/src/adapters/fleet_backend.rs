use std::sync::Arc;

use crate::adapters::{
    InMemoryFleetRegistry, InMemoryWorkQueue, RedisFleetRegistry, RedisStreamQueue,
};
use crate::ports::{FleetRegistry, WorkQueue};

pub struct FleetBackend {
    pub queue: Arc<dyn WorkQueue>,
    pub registry: Arc<dyn FleetRegistry>,
}

#[derive(Debug, thiserror::Error)]
pub enum FleetBackendError {
    #[error("connect fleet Redis Streams queue")]
    Queue(#[source] redis::RedisError),
    #[error("connect fleet Redis registry")]
    Registry(#[source] redis::RedisError),
}

impl FleetBackend {
    pub async fn connect(
        redis_url: Option<&str>,
        consumer: &str,
    ) -> Result<Self, FleetBackendError> {
        match redis_url {
            Some(url) => Ok(Self {
                queue: RedisStreamQueue::connect(url, consumer)
                    .await
                    .map_err(FleetBackendError::Queue)?,
                registry: RedisFleetRegistry::connect(url)
                    .await
                    .map_err(FleetBackendError::Registry)?,
            }),
            None => {
                Ok(Self { queue: InMemoryWorkQueue::new(), registry: InMemoryFleetRegistry::new() })
            }
        }
    }
}
