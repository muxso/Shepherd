//! 编排所需的 gateway 端口(由组装根接到 task / verification 的真实服务)。

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrchError {
    #[error("gateway error: {0}")]
    Gateway(String),
}

/// 任务拆分图 → 它归属的需求版本。
#[async_trait]
pub trait DecompositionGateway: Send + Sync {
    /// 拆分图归属的 (requirement_id, version);不存在返回 None。
    async fn requirement_of(
        &self,
        decomposition_id: &str,
    ) -> Result<Option<(String, u32)>, OrchError>;
}

/// 验证侧:查某需求版本的验证 + 同步任务的覆盖链状态。
#[async_trait]
pub trait VerificationGateway: Send + Sync {
    /// 某需求版本的验证 id;未开验证返回 None。
    async fn find_verification(
        &self,
        requirement_id: &str,
        version: u32,
    ) -> Result<Option<String>, OrchError>;

    /// 把某任务的交付验证状态同步进验证覆盖链。
    async fn sync(
        &self,
        verification_id: &str,
        decomposition_id: &str,
        task_id: &str,
        satisfied: bool,
    ) -> Result<(), OrchError>;
}
