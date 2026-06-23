//! 资源池管理端口:创建 / 列出 / 取单 / 更新 / 删除。
//!
//! 与只读的 `ResourcePoolPort`(批量运行时解析/校验池)分开:这里是管理面,
//! 给 CLI / UI 创建·浏览·维护池用。沿用 `PortError` 统一后端错误。

use async_trait::async_trait;

use crate::domain::{NewResourcePool, ResourcePool};
use crate::ports::PortError;

#[async_trait]
pub trait ResourcePoolAdminPort: Send + Sync {
    /// 创建资源池,返回含生成 id 的视图。
    async fn create(&self, pool: &NewResourcePool) -> Result<ResourcePool, PortError>;

    /// 列出全部未删除的资源池(最新创建在前)。
    async fn list(&self) -> Result<Vec<ResourcePool>, PortError>;

    /// 按 id 取单个未删除资源池;不存在返回 None。
    async fn get(&self, id: &str) -> Result<Option<ResourcePool>, PortError>;

    /// 全量更新资源池(含启停);目标不存在返回 None。
    async fn update(&self, id: &str, pool: &NewResourcePool) -> Result<Option<ResourcePool>, PortError>;

    /// 软删除资源池;命中返回 true,目标不存在返回 false。
    async fn delete(&self, id: &str) -> Result<bool, PortError>;
}
