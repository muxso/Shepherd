//! 机群注册表适配器。`InMemoryFleetRegistry`(单机)始终编译;Redis 版见 `redis_registry`。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use crate::ports::{FleetRegistry, RuntimeInfo, ONLINE_TTL_MS};

pub(crate) fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

struct Entry {
    name: String,
    caps: Vec<String>,
    max_concurrency: u32,
    last_seen_ms: u64,
}

/// 进程内注册表(单机/测试)。多副本请用 `RedisFleetRegistry`。
#[derive(Default)]
pub struct InMemoryFleetRegistry {
    seq: AtomicU64,
    rts: Mutex<HashMap<String, Entry>>,
}

impl InMemoryFleetRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[async_trait]
impl FleetRegistry for InMemoryFleetRegistry {
    async fn register(&self, name: &str, caps: &[String], max_concurrency: u32) -> String {
        let id = format!("rt-{}", self.seq.fetch_add(1, Ordering::Relaxed) + 1);
        self.rts.lock().unwrap().insert(
            id.clone(),
            Entry {
                name: name.to_string(),
                caps: caps.to_vec(),
                max_concurrency,
                last_seen_ms: now_ms(),
            },
        );
        id
    }

    async fn heartbeat(&self, id: &str) -> bool {
        match self.rts.lock().unwrap().get_mut(id) {
            Some(e) => {
                e.last_seen_ms = now_ms();
                true
            }
            None => false,
        }
    }

    async fn list(&self) -> Vec<RuntimeInfo> {
        let now = now_ms();
        let mut out: Vec<RuntimeInfo> = self
            .rts
            .lock()
            .unwrap()
            .iter()
            .map(|(id, e)| RuntimeInfo {
                id: id.clone(),
                name: e.name.clone(),
                caps: e.caps.clone(),
                max_concurrency: e.max_concurrency,
                last_seen_ms: e.last_seen_ms,
                online: now.saturating_sub(e.last_seen_ms) < ONLINE_TTL_MS,
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_heartbeat_list() {
        let r = InMemoryFleetRegistry::new();
        let id = r.register("box-1", &["CLAUDE_CODE".into()], 2).await;
        assert!(r.heartbeat(&id).await);
        assert!(!r.heartbeat("ghost").await);
        let list = r.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "box-1");
        assert!(list[0].online); // 刚心跳过
        assert_eq!(list[0].caps, vec!["CLAUDE_CODE".to_string()]);
    }
}
