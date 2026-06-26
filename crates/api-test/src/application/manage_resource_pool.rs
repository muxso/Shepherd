use std::sync::Arc;

use thiserror::Error;

use crate::domain::{NewResourcePool, ResourcePool, ResourcePoolDraft, ResourcePoolError};
use crate::ports::{PortError, ResourcePoolAdminPort};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateResourcePoolError {
    #[error(transparent)]
    Validation(#[from] ResourcePoolError),
    #[error(transparent)]
    Backend(#[from] PortError),
}

#[derive(Clone)]
pub struct CreateResourcePoolUseCase {
    admin: Arc<dyn ResourcePoolAdminPort>,
}

impl CreateResourcePoolUseCase {
    pub fn new(admin: Arc<dyn ResourcePoolAdminPort>) -> Self {
        Self { admin }
    }

    pub async fn execute(&self, draft: ResourcePoolDraft) -> Result<ResourcePool, CreateResourcePoolError> {
        let new_pool = NewResourcePool::new(draft)?;
        Ok(self.admin.create(&new_pool).await?)
    }
}

#[derive(Clone)]
pub struct ListResourcePoolsUseCase {
    admin: Arc<dyn ResourcePoolAdminPort>,
}

impl ListResourcePoolsUseCase {
    pub fn new(admin: Arc<dyn ResourcePoolAdminPort>) -> Self {
        Self { admin }
    }

    pub async fn execute(&self) -> Result<Vec<ResourcePool>, PortError> {
        self.admin.list().await
    }
}

#[derive(Clone)]
pub struct EditResourcePoolUseCase {
    admin: Arc<dyn ResourcePoolAdminPort>,
}

impl EditResourcePoolUseCase {
    pub fn new(admin: Arc<dyn ResourcePoolAdminPort>) -> Self {
        Self { admin }
    }

    pub async fn get(&self, id: &str) -> Result<Option<ResourcePool>, PortError> {
        self.admin.get(id).await
    }

    pub async fn update(
        &self,
        id: &str,
        draft: ResourcePoolDraft,
    ) -> Result<Option<ResourcePool>, CreateResourcePoolError> {
        let new_pool = NewResourcePool::new(draft)?;
        Ok(self.admin.update(id, &new_pool).await?)
    }

    pub async fn delete(&self, id: &str) -> Result<bool, PortError> {
        self.admin.delete(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    fn draft(name: &str) -> ResourcePoolDraft {
        ResourcePoolDraft { name: name.to_string(), enabled: true, all_org: true, ..Default::default() }
    }

    #[derive(Default)]
    struct FakeAdmin {
        pools: Mutex<Vec<ResourcePool>>,
    }

    fn view(id: String, p: &NewResourcePool) -> ResourcePool {
        ResourcePool {
            id,
            name: p.name.clone(),
            enabled: p.enabled,
            description: p.description.clone(),
            max_concurrency: p.max_concurrency,
            pool_type: p.pool_type.clone(),
            all_org: p.all_org,
            org_ids: p.org_ids.clone(),
            server_url: p.server_url.clone(),
            config: p.config.clone(),
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
        }
    }

    #[async_trait]
    impl ResourcePoolAdminPort for FakeAdmin {
        async fn create(&self, pool: &NewResourcePool) -> Result<ResourcePool, PortError> {
            let mut pools = self.pools.lock().expect("lock");
            let v = view(format!("p{}", pools.len() + 1), pool);
            pools.push(v.clone());
            Ok(v)
        }
        async fn list(&self) -> Result<Vec<ResourcePool>, PortError> {
            Ok(self.pools.lock().expect("lock").clone())
        }
        async fn get(&self, id: &str) -> Result<Option<ResourcePool>, PortError> {
            Ok(self.pools.lock().expect("lock").iter().find(|p| p.id == id).cloned())
        }
        async fn update(&self, id: &str, pool: &NewResourcePool) -> Result<Option<ResourcePool>, PortError> {
            let mut pools = self.pools.lock().expect("lock");
            match pools.iter_mut().find(|p| p.id == id) {
                Some(slot) => {
                    *slot = view(id.to_string(), pool);
                    Ok(Some(slot.clone()))
                }
                None => Ok(None),
            }
        }
        async fn delete(&self, id: &str) -> Result<bool, PortError> {
            let mut pools = self.pools.lock().expect("lock");
            let before = pools.len();
            pools.retain(|p| p.id != id);
            Ok(pools.len() != before)
        }
    }

    #[tokio::test]
    async fn creates_then_lists() {
        let admin = Arc::new(FakeAdmin::default());
        let create = CreateResourcePoolUseCase::new(admin.clone());
        let list = ListResourcePoolsUseCase::new(admin);

        let p = create.execute(draft("  本地池 ")).await.expect("created");
        assert_eq!(p.id, "p1");
        assert_eq!(p.name, "本地池");
        assert!(p.enabled);

        create.execute(draft("远端池")).await.expect("created");
        let all = list.execute().await.expect("listed");
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn rejects_blank_name_before_backend() {
        let admin = Arc::new(FakeAdmin::default());
        let create = CreateResourcePoolUseCase::new(admin.clone());
        let err = create.execute(draft("   ")).await.unwrap_err();
        assert_eq!(err, CreateResourcePoolError::Validation(ResourcePoolError::EmptyName));
        assert!(admin.list().await.expect("listed").is_empty());
    }

    #[tokio::test]
    async fn updates_and_deletes() {
        let admin = Arc::new(FakeAdmin::default());
        let create = CreateResourcePoolUseCase::new(admin.clone());
        let edit = EditResourcePoolUseCase::new(admin.clone());

        let p = create.execute(draft("池")).await.expect("created");
        let updated = edit
            .update(&p.id, ResourcePoolDraft { enabled: false, ..draft("池改名") })
            .await
            .expect("ok")
            .expect("found");
        assert_eq!(updated.name, "池改名");
        assert!(!updated.enabled);

        assert!(edit.update("nope", draft("x")).await.expect("ok").is_none());
        assert!(!edit.delete("nope").await.expect("ok"));

        assert!(edit.delete(&p.id).await.expect("ok"));
        assert!(admin.list().await.expect("listed").is_empty());
    }
}
