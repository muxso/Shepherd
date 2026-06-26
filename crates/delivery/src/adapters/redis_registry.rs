//! 在线状态由心跳新鲜度在 list 时算出,server 不反向探活。

use std::sync::Arc;

use async_trait::async_trait;
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;

use crate::adapters::fleet_registry::now_ms;
use crate::ports::{FleetRegistry, RuntimeInfo, ONLINE_TTL_MS};

const IDS_SET: &str = "fleet:rt:ids";
const SEQ: &str = "fleet:rt:seq";

fn rt_key(id: &str) -> String {
    format!("fleet:rt:{id}")
}

pub struct RedisFleetRegistry {
    conn: MultiplexedConnection,
}

impl RedisFleetRegistry {
    pub async fn connect(url: &str) -> Result<Arc<Self>, redis::RedisError> {
        let client = redis::Client::open(url)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Arc::new(Self { conn }))
    }
}

#[async_trait]
impl FleetRegistry for RedisFleetRegistry {
    async fn register(&self, name: &str, caps: &[String], max_concurrency: u32) -> String {
        let mut conn = self.conn.clone();
        let seq: i64 = conn.incr(SEQ, 1).await.unwrap_or(0);
        let id = format!("rt-{seq}");
        let key = rt_key(&id);
        let fields = [
            ("name", name.to_string()),
            ("caps", caps.join(",")),
            ("max", max_concurrency.to_string()),
            ("last_seen", now_ms().to_string()),
        ];
        let _: redis::RedisResult<()> = conn.hset_multiple(&key, &fields).await;
        let _: redis::RedisResult<()> = conn.sadd(IDS_SET, &id).await;
        id
    }

    async fn heartbeat(&self, id: &str) -> bool {
        let mut conn = self.conn.clone();
        let exists: bool = conn.exists(rt_key(id)).await.unwrap_or(false);
        if !exists {
            return false;
        }
        let _: redis::RedisResult<()> = conn.hset(rt_key(id), "last_seen", now_ms()).await;
        true
    }

    async fn list(&self) -> Vec<RuntimeInfo> {
        let mut conn = self.conn.clone();
        let ids: Vec<String> = conn.smembers(IDS_SET).await.unwrap_or_default();
        let now = now_ms();
        let mut out = Vec::new();
        for id in ids {
            let map: std::collections::HashMap<String, String> =
                match conn.hgetall(rt_key(&id)).await {
                    Ok(m) => m,
                    Err(_) => continue,
                };
            if map.is_empty() {
                continue;
            }
            let last_seen_ms = map.get("last_seen").and_then(|s| s.parse().ok()).unwrap_or(0);
            out.push(RuntimeInfo {
                id: id.clone(),
                name: map.get("name").cloned().unwrap_or_default(),
                caps: map
                    .get("caps")
                    .map(|c| c.split(',').filter(|s| !s.is_empty()).map(String::from).collect())
                    .unwrap_or_default(),
                max_concurrency: map.get("max").and_then(|s| s.parse().ok()).unwrap_or(1),
                last_seen_ms,
                online: now.saturating_sub(last_seen_ms) < ONLINE_TTL_MS,
            });
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }
}
