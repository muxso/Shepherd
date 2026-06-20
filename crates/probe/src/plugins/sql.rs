//! SQL 协议插件:target=连接串,payload=语句。输出=受影响行数,status=0(OK)/None(失败)。
//!
//! 按连接串缓存连接池:一次性探测与高并发压测都复用同一池(压测时不会每请求重连,
//! 真正测的是目标库吞吐而非连接开销)。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sqlx::PgPool;

use crate::domain::{ProbeRequest, RawProbe};
use crate::ports::ProtocolPlugin;

#[derive(Default)]
pub struct SqlPlugin {
    /// 连接串 → 池(复用,避免每次探测/压测重连)。
    pools: Mutex<HashMap<String, PgPool>>,
}

impl SqlPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// 取目标的连接池(已有则复用;否则新建并缓存)。连接在锁外建立,避免阻塞其它目标。
    async fn pool_for(&self, target: &str) -> Result<PgPool, String> {
        if let Some(p) = self.pools.lock().expect("pools lock").get(target).cloned() {
            return Ok(p);
        }
        // max_connections=16:够支撑中等并发压测;一次性探测也无妨。
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(16)
            .acquire_timeout(Duration::from_secs(5))
            .connect(target)
            .await
            .map_err(|e| e.to_string())?;
        // 竞态下若已有他人插入,用既有的(丢弃本次新建)。
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
impl ProtocolPlugin for SqlPlugin {
    fn protocol(&self) -> &'static str {
        "sql"
    }

    async fn run(&self, req: &ProbeRequest) -> RawProbe {
        let query = req.payload.clone().unwrap_or_else(|| "SELECT 1".to_string());
        let pool = match self.pool_for(&req.target).await {
            Ok(p) => p,
            Err(e) => {
                return RawProbe {
                    transport_ok: false,
                    error: Some(e),
                    ..Default::default()
                }
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
