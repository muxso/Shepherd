use async_trait::async_trait;

use crate::domain::{ProbeRequest, RawProbe};

#[async_trait]
pub trait ProtocolPlugin: Send + Sync {
    fn protocol(&self) -> &'static str;
    async fn run(&self, req: &ProbeRequest) -> RawProbe;
}
