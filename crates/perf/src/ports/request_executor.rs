use async_trait::async_trait;

#[async_trait]
pub trait RequestExecutor: Send + Sync {
    async fn execute(&self) -> bool;
}
