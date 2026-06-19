//! 资源池管理端口:创建 + 列出。
//!
//! 与只读的 `ResourcePoolPort`(批量运行时解析/校验池)分开:这里是管理面,
//! 给 CLI / UI 创建和浏览池用。沿用 `PortError` 统一后端错误。

use async_trait::async_trait;

use crate::domain::{NewResourcePool, ResourcePool};
use crate::ports::PortError;

#[async_trait]
pub trait ResourcePoolAdminPort: Send + Sync {
    /// 创建资源池,返回含生成 id 的视图。
    async fn create(&self, pool: &NewResourcePool) -> Result<ResourcePool, PortError>;

    /// 列出全部未删除的资源池(按 name 排序)。
    async fn list(&self) -> Result<Vec<ResourcePool>, PortError>;
}
