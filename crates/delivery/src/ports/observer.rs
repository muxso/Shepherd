//! 交付观察者出站端口(可选):一次尝试进入 Running/Delivered/Failed 时被通知。
//! 组装根可把它桥接到 orchestrator,实现"驱动任务生命周期 + 交付结果回灌验证"。
//! delivery 本身不依赖编排器。

use async_trait::async_trait;

use crate::domain::DeliveryAttempt;

#[async_trait]
pub trait DeliveryObserver: Send + Sync {
    /// 尝试进入非初始状态(Running / Delivered / Failed)。实现应自行吞掉错误(尽力而为)。
    async fn on_progress(&self, attempt: &DeliveryAttempt);
}
