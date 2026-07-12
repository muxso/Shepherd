//! Runtimes have no inbound network access, so the server cannot probe them; online
//! status is determined entirely by heartbeat freshness.

use async_trait::async_trait;

pub const ONLINE_TTL_MS: u64 = 30_000;

#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    pub id: String,
    pub name: String,
    pub caps: Vec<String>,
    pub max_concurrency: u32,
    pub last_seen_ms: u64,
    pub online: bool,
}

#[async_trait]
pub trait FleetRegistry: Send + Sync {
    async fn register(&self, name: &str, caps: &[String], max_concurrency: u32) -> String;
    async fn heartbeat(&self, id: &str) -> bool;
    async fn list(&self) -> Vec<RuntimeInfo>;
}
