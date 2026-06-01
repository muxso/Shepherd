//! 执行事件回流端口:执行者在运行中把决策/文件变更/测试结果等事件 emit 出来,
//! 由 `DeliveryService` 绑定到当前交付尝试并落库(尽力而为)。

use async_trait::async_trait;

use crate::domain::NewExecutionEvent;

#[async_trait]
pub trait EventSink: Send + Sync {
    /// 上报一条执行事件。实现应自行吞掉错误(不影响执行主流程)。
    async fn emit(&self, event: NewExecutionEvent);
}

/// 丢弃所有事件(执行者单测/无需审计时使用)。
pub struct NoopEventSink;

#[async_trait]
impl EventSink for NoopEventSink {
    async fn emit(&self, _event: NewExecutionEvent) {}
}
