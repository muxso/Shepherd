use redis::AsyncCommands;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Weak,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

/// Redis is optional: failures fall back to the compact PostgreSQL read model.
pub struct StatisticsCache {
    client: redis::Client,
    connection: Mutex<Option<redis::aio::MultiplexedConnection>>,
    retry_at: AtomicU64,
    flights: Mutex<HashMap<String, Weak<Mutex<()>>>>,
}
impl StatisticsCache {
    pub fn new(url: &str) -> Option<Arc<Self>> {
        let client = redis::Client::open(url).ok()?;
        Some(Arc::new(Self {
            client,
            connection: Mutex::new(None),
            retry_at: AtomicU64::new(0),
            flights: Mutex::new(HashMap::new()),
        }))
    }
    pub fn key(source: &str, revision: i64, day: i64, endpoint: &str, query: &str) -> String {
        let digest = Sha256::digest(
            serde_json::to_vec(&(source, revision, day, endpoint, query)).unwrap_or_default(),
        );
        format!("shepherd:yunxiao:statistics:v1:{digest:x}")
    }
    async fn connection(&self) -> Option<redis::aio::MultiplexedConnection> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        if self.retry_at.load(Ordering::Relaxed) > now {
            return None;
        }
        let mut connection = self.connection.lock().await;
        if let Some(c) = connection.as_ref() {
            return Some(c.clone());
        }
        match tokio::time::timeout(
            Duration::from_millis(150),
            self.client.get_multiplexed_async_connection(),
        )
        .await
        {
            Ok(Ok(c)) => {
                *connection = Some(c.clone());
                Some(c)
            }
            _ => {
                self.retry_at.store(now + 10, Ordering::Relaxed);
                None
            }
        }
    }
    pub async fn get(&self, key: &str) -> Option<Value> {
        let mut c = self.connection().await?;
        match tokio::time::timeout(Duration::from_millis(150), c.get::<_, Option<Vec<u8>>>(key))
            .await
        {
            Ok(Ok(Some(raw))) => serde_json::from_slice(&raw).ok(),
            Ok(Ok(None)) => None,
            _ => {
                self.broken().await;
                None
            }
        }
    }
    pub async fn set(&self, key: &str, value: &Value) {
        self.set_for(key, value, 90).await;
    }
    pub async fn set_for(&self, key: &str, value: &Value, ttl: u64) {
        let Ok(raw) = serde_json::to_vec(value) else { return };
        if raw.len() > 4_000_000 {
            return;
        }
        let Some(mut c) = self.connection().await else { return };
        if !matches!(
            tokio::time::timeout(Duration::from_millis(150), c.set_ex::<_, _, ()>(key, raw, ttl))
                .await,
            Ok(Ok(()))
        ) {
            self.broken().await;
        }
    }
    async fn broken(&self) {
        *self.connection.lock().await = None;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        self.retry_at.store(now + 10, Ordering::Relaxed);
    }
    pub async fn flight(&self, key: &str) -> Arc<Mutex<()>> {
        let mut flights = self.flights.lock().await;
        if let Some(lock) = flights.get(key).and_then(Weak::upgrade) {
            return lock;
        }
        flights.retain(|_, v| v.strong_count() > 0);
        let lock = Arc::new(Mutex::new(()));
        flights.insert(key.into(), Arc::downgrade(&lock));
        lock
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_separate_sources_revisions_time_filters_and_permission_scopes() {
        let base = StatisticsCache::key("source-a", 1, 100, "activity", "code/true");
        for key in [
            StatisticsCache::key("source-b", 1, 100, "activity", "code/true"),
            StatisticsCache::key("source-a", 2, 100, "activity", "code/true"),
            StatisticsCache::key("source-a", 1, 101, "activity", "code/true"),
            StatisticsCache::key("source-a", 1, 100, "activity", "code/false"),
            StatisticsCache::key("source-a", 1, 100, "activity", "test/true"),
        ] {
            assert_ne!(base, key);
        }
        assert!(!base.contains("source-a"));
    }
    #[tokio::test]
    async fn unavailable_cache_is_bounded_and_recoverable_by_the_database_path() {
        let cache = StatisticsCache::new("redis://127.0.0.1:1/").expect("cache config");
        let start = std::time::Instant::now();
        assert!(cache.get("not-present").await.is_none());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
