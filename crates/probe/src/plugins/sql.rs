//! SQL 协议插件:target=连接串,payload=语句。输出=受影响行数,status=0(OK)/None(失败)。

use async_trait::async_trait;
use std::time::Instant;

use crate::domain::{ProbeRequest, RawProbe};
use crate::ports::ProtocolPlugin;

pub struct SqlPlugin;

impl Default for SqlPlugin {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl ProtocolPlugin for SqlPlugin {
    fn protocol(&self) -> &'static str {
        "sql"
    }

    async fn run(&self, req: &ProbeRequest) -> RawProbe {
        let query = req.payload.clone().unwrap_or_else(|| "SELECT 1".to_string());
        let pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(&req.target)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                return RawProbe {
                    transport_ok: false,
                    error: Some(e.to_string()),
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
