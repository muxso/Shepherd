//! runtime 无公网入站,server 无法反向探活,故在线状态全由心跳新鲜度判定。

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
