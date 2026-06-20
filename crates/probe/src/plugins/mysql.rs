//! MySQL 协议插件:target=连接串(mysql://user:pass@host:port/db),payload=语句。
//! 输出=受影响行数,status=0(OK)/None(失败)。
//!
//! 按连接串缓存连接池:一次性探测与高并发压测都复用同一池(压测时不每请求重连,
//! 测的是目标库吞吐而非连接开销)。与 sql(postgres)插件同构,仅驱动不同。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sqlx::MySqlPool;

use crate::domain::{ProbeRequest, RawProbe};
use crate::ports::ProtocolPlugin;

#[derive(Default)]
pub struct MysqlPlugin {
    /// 连接串 → 池(复用,避免每次探测/压测重连)。
    pools: Mutex<HashMap<String, MySqlPool>>,
}

impl MysqlPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// 取目标的连接池(已有则复用;否则新建并缓存)。连接在锁外建立。
    async fn pool_for(&self, target: &str) -> Result<MySqlPool, String> {
        if let Some(p) = self.pools.lock().expect("pools lock").get(target).cloned() {
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
            .expect("pools lock")
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
        // 连不上的目标 → transport_ok=false(就地执行、如实回传)。
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
