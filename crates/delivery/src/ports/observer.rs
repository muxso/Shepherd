//! 交付观察者出站端口(可选):一次尝试进入终态(Delivered/Failed)后被通知。
//! 组装根可把它桥接到 orchestrator,实现"交付结果自动回灌验证"。delivery 本身不依赖编排器。

use async_trait::async_trait;

use crate::domain::DeliveryAttempt;

#[async_trait]
pub trait DeliveryObserver: Send + Sync {
    /// 尝试已落终态(Delivered 或 Failed)。实现应自行吞掉错误(尽力而为,不影响交付主流程)。
    async fn on_settled(&self, attempt: &DeliveryAttempt);
}
