use crate::domain::*;
use async_trait::async_trait;

/// Every operation is tenant scoped, including lookup/delete by an otherwise opaque id.
#[async_trait]
pub trait ProfileStore: Send + Sync {
    async fn create(&self, project: &str, profile: NewProfile) -> Result<Profile, StoreError>;
    async fn get(&self, project: &str, id: &str) -> Result<Profile, StoreError>;
    async fn list(&self, project: &str) -> Result<Vec<Profile>, StoreError>;
    async fn update(
        &self,
        project: &str,
        id: &str,
        revision: i64,
        profile: NewProfile,
    ) -> Result<Profile, StoreError>;
    async fn delete(&self, project: &str, id: &str, revision: i64) -> Result<(), StoreError>;
    async fn bind(&self, binding: Binding) -> Result<(), StoreError>;
    async fn bindings(&self, project: &str, environment: &str) -> Result<Vec<Binding>, StoreError>;
    async fn unbind(
        &self,
        project: &str,
        environment: &str,
        slot: BindingSlot,
        profile: &str,
    ) -> Result<(), StoreError>;
}
