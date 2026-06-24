//! 拆分触发出站端口:设计稿审批通过后,放行 Shepherd 的任务拆分(Design 阶段 → 拆分)。
//!
//! design 上下文不直接依赖 task/decomposition;由组装根把本端口桥接到拆分用例
//! (`POST /requirement/{id}/breakdown` 或 decomposition 服务),实现「批准即开拆」。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::Proposal;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TriggerError {
    #[error("breakdown trigger error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait BreakdownTrigger: Send + Sync {
    /// 设计稿审批通过 → 放行该需求的任务拆分(尽力而为,失败不回滚审批)。
    async fn on_design_approved(&self, proposal: &Proposal) -> Result<(), TriggerError>;
}
