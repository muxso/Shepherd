use async_trait::async_trait;

use crate::domain::{NewResourcePool, ResourcePool};
use crate::ports::PortError;

#[async_trait]
pub trait ResourcePoolAdminPort: Send + Sync {
    async fn create(&self, pool: &NewResourcePool) -> Result<ResourcePool, PortError>;

    async fn list(&self) -> Result<Vec<ResourcePool>, PortError>;

    async fn get(&self, id: &str) -> Result<Option<ResourcePool>, PortError>;

    async fn update(
        &self,
        id: &str,
        pool: &NewResourcePool,
    ) -> Result<Option<ResourcePool>, PortError>;

    async fn delete(&self, id: &str) -> Result<bool, PortError>;
}
