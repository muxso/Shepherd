//! 用例:创建 / 列出资源池。
//!
//! 创建走领域校验(名称非空)→ 管理端口落库;列出直接透传端口。
//! 错误区分校验失败(→ 400)与后端错误(→ 500)。

use std::sync::Arc;

use thiserror::Error;

use crate::domain::{NewResourcePool, ResourcePool, ResourcePoolError};
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

    pub async fn execute(
        &self,
        name: &str,
        enabled: bool,
    ) -> Result<ResourcePool, CreateResourcePoolError> {
        let new_pool = NewResourcePool::new(name, enabled)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// 内存假实现:create 生成确定性 id(p{N}),list 按 name 排序返回。
    #[derive(Default)]
    struct FakeAdmin {
        pools: Mutex<Vec<ResourcePool>>,
    }

    #[async_trait]
    impl ResourcePoolAdminPort for FakeAdmin {
        async fn create(&self, pool: &NewResourcePool) -> Result<ResourcePool, PortError> {
            let mut pools = self.pools.lock().expect("lock");
            let view = ResourcePool {
                id: format!("p{}", pools.len() + 1),
                name: pool.name.clone(),
                enabled: pool.enabled,
            };
            pools.push(view.clone());
            Ok(view)
        }
        async fn list(&self) -> Result<Vec<ResourcePool>, PortError> {
            let mut out = self.pools.lock().expect("lock").clone();
            out.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(out)
        }
    }

    #[tokio::test]
    async fn creates_then_lists() {
        let admin = Arc::new(FakeAdmin::default());
        let create = CreateResourcePoolUseCase::new(admin.clone());
        let list = ListResourcePoolsUseCase::new(admin);

        let p = create.execute("  本地池 ", true).await.expect("created");
        assert_eq!(p.id, "p1");
        assert_eq!(p.name, "本地池"); // 去空白
        assert!(p.enabled);

        create.execute("远端池", true).await.expect("created");
        let all = list.execute().await.expect("listed");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "本地池"); // 按 name 排序
    }

    #[tokio::test]
    async fn rejects_blank_name_before_backend() {
        let admin = Arc::new(FakeAdmin::default());
        let create = CreateResourcePoolUseCase::new(admin.clone());
        let err = create.execute("   ", true).await.unwrap_err();
        assert_eq!(err, CreateResourcePoolError::Validation(ResourcePoolError::EmptyName));
        // 校验失败不应落库。
        assert!(admin.list().await.expect("listed").is_empty());
    }
}
