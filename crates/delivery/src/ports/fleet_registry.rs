//! 执行者机群注册表端口:runtime 出站 register/heartbeat,server 据心跳判活。
//!
//! agent 无公网入站,server **不能反向探活**(对比 runner-agent 注册时拉 /protocols),
//! 故在线状态完全由 runtime 周期心跳驱动:`online = now - last_seen < ONLINE_TTL`。

use async_trait::async_trait;

/// 心跳新鲜度阈值:超过即判离线。
pub const ONLINE_TTL_MS: u64 = 30_000;

/// 已登记的 runtime 视图。
#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    pub id: String,
    pub name: String,
    pub caps: Vec<String>,
    pub max_concurrency: u32,
    pub last_seen_ms: u64,
    /// 由 `last_seen_ms` 与当前时间在读取时算出。
    pub online: bool,
}

#[async_trait]
pub trait FleetRegistry: Send + Sync {
    /// 登记一台 runtime(出站 register),返回其 id。
    async fn register(&self, name: &str, caps: &[String], max_concurrency: u32) -> String;
    /// 心跳续约(出站 heartbeat),刷新 last_seen。未知 id 返回 false。
    async fn heartbeat(&self, id: &str) -> bool;
    /// 列出全部 runtime(含在线判定),供 Web 机群视图。
    async fn list(&self) -> Vec<RuntimeInfo>;
}
