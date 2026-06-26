use async_trait::async_trait;

use crate::domain::DeliveryAttempt;

#[async_trait]
pub trait DeliveryObserver: Send + Sync {
    async fn on_progress(&self, attempt: &DeliveryAttempt);
}
