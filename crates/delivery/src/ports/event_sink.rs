use async_trait::async_trait;

use crate::domain::NewExecutionEvent;

#[async_trait]
pub trait EventSink: Send + Sync {
    async fn emit(&self, event: NewExecutionEvent);
}

pub struct NoopEventSink;

#[async_trait]
impl EventSink for NoopEventSink {
    async fn emit(&self, _event: NewExecutionEvent) {}
}
