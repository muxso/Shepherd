use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sqlx::MySqlPool;

use crate::domain::{ProbeRequest, RawProbe};
use crate::ports::ProtocolPlugin;

#[derive(Default)]
pub struct MysqlPlugin {
    pools: Mutex<HashMap<String, MySqlPool>>,
}

impl MysqlPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    async fn pool_for(&self, target: &str) -> Result<MySqlPool, String> {
        if let Some(p) = self
            .pools
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(target)
            .cloned()
        {
            return Ok(p);
        }
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(16)
            .acquire_timeout(Duration::from_secs(5))
            .connect(target)
            .await
            .map_err(|e| e.to_string())?;
        Ok(self
            .pools
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(target.to_string())
            .or_insert(pool)
            .clone())
    }
}

#[async_trait]
impl ProtocolPlugin for MysqlPlugin {
    fn protocol(&self) -> &'static str {
        "mysql"
    }

    async fn run(&self, req: &ProbeRequest) -> RawProbe {
        let query = req.payload.clone().unwrap_or_else(|| "SELECT 1".to_string());
        let pool = match self.pool_for(&req.target).await {
            Ok(p) => p,
            Err(e) => {
                return RawProbe { transport_ok: false, error: Some(e), ..Default::default() }
            }
        };
        let t = Instant::now();
        match sqlx::query(&query).execute(&pool).await {
            Ok(r) => RawProbe {
                transport_ok: true,
                status: Some(0),
                latency_ms: t.elapsed().as_millis() as u64,
                output: Some(format!("rows_affected={}", r.rows_affected())),
                error: None,
            },
            Err(e) => RawProbe {
                transport_ok: false,
                latency_ms: t.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
                ..Default::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unreachable_target_is_transport_failure() {
        let plugin = MysqlPlugin::new();
        let req = ProbeRequest {
            protocol: "mysql".into(),
            target: "mysql://root:bad@127.0.0.1:1/nope".into(),
            payload: Some("SELECT 1".into()),
            metadata: Default::default(),
            assertions: vec![],
        };
        let raw = plugin.run(&req).await;
        assert!(!raw.transport_ok);
        assert!(raw.error.is_some());
    }
}
