use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use redis::aio::MultiplexedConnection;
use redis::Value;

use crate::domain::{ProbeRequest, RawProbe};
use crate::ports::ProtocolPlugin;

#[derive(Default)]
pub struct RedisPlugin {
    conns: Mutex<HashMap<String, MultiplexedConnection>>,
}

impl RedisPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    async fn conn_for(&self, target: &str) -> Result<MultiplexedConnection, String> {
        if let Some(c) = self.conns.lock().expect("conns lock").get(target).cloned() {
            return Ok(c);
        }
        let client = redis::Client::open(target).map_err(|e| e.to_string())?;
        let conn =
            client.get_multiplexed_async_connection().await.map_err(|e| e.to_string())?;
        Ok(self
            .conns
            .lock()
            .expect("conns lock")
            .entry(target.to_string())
            .or_insert(conn)
            .clone())
    }
}

fn format_value(v: &Value) -> String {
    match v {
        Value::Nil => "nil".to_string(),
        Value::Int(i) => i.to_string(),
        Value::BulkString(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        Value::SimpleString(s) => s.clone(),
        Value::Okay => "OK".to_string(),
        Value::Array(items) | Value::Set(items) => {
            items.iter().map(format_value).collect::<Vec<_>>().join(",")
        }
        Value::Double(d) => d.to_string(),
        Value::Boolean(b) => b.to_string(),
        other => format!("{other:?}"),
    }
}

#[async_trait]
impl ProtocolPlugin for RedisPlugin {
    fn protocol(&self) -> &'static str {
        "redis"
    }

    async fn run(&self, req: &ProbeRequest) -> RawProbe {
        let line = req.payload.clone().unwrap_or_else(|| "PING".to_string());
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else {
            return RawProbe {
                transport_ok: false,
                error: Some("empty redis command".into()),
                ..Default::default()
            };
        };
        let mut cmd = redis::cmd(name);
        for arg in parts {
            cmd.arg(arg);
        }

        let mut conn = match self.conn_for(&req.target).await {
            Ok(c) => c,
            Err(e) => {
                return RawProbe { transport_ok: false, error: Some(e), ..Default::default() }
            }
        };
        let t = Instant::now();
        match cmd.query_async::<Value>(&mut conn).await {
            Ok(v) => RawProbe {
                transport_ok: true,
                status: Some(0),
                latency_ms: t.elapsed().as_millis() as u64,
                output: Some(format_value(&v)),
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

    #[test]
    fn formats_common_replies() {
        assert_eq!(format_value(&Value::Okay), "OK");
        assert_eq!(format_value(&Value::SimpleString("PONG".into())), "PONG");
        assert_eq!(format_value(&Value::Int(42)), "42");
        assert_eq!(format_value(&Value::BulkString(b"hello".to_vec())), "hello");
        assert_eq!(format_value(&Value::Nil), "nil");
        assert_eq!(
            format_value(&Value::Array(vec![Value::Int(1), Value::BulkString(b"a".to_vec())])),
            "1,a"
        );
    }

    #[tokio::test]
    async fn unreachable_target_is_transport_failure() {
        let plugin = RedisPlugin::new();
        let req = ProbeRequest {
            protocol: "redis".into(),
            target: "redis://127.0.0.1:1/0".into(),
            payload: Some("PING".into()),
            metadata: Default::default(),
            assertions: vec![],
        };
        let raw = plugin.run(&req).await;
        assert!(!raw.transport_ok);
        assert!(raw.error.is_some());
    }
}
