use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::domain::{ProbeRequest, RawProbe};
use crate::ports::ProtocolPlugin;

#[derive(Default)]
pub struct WebSocketPlugin;

impl WebSocketPlugin {
    pub fn new() -> Self {
        Self
    }
}

fn message_to_string(msg: &Message) -> String {
    match msg {
        Message::Text(t) => t.to_string(),
        Message::Binary(b) => String::from_utf8_lossy(b).into_owned(),
        Message::Ping(_) => "<ping>".to_string(),
        Message::Pong(_) => "<pong>".to_string(),
        Message::Close(_) => "<close>".to_string(),
        Message::Frame(_) => "<frame>".to_string(),
    }
}

#[async_trait]
impl ProtocolPlugin for WebSocketPlugin {
    fn protocol(&self) -> &'static str {
        "websocket"
    }

    async fn run(&self, req: &ProbeRequest) -> RawProbe {
        let t = Instant::now();
        let (mut ws, _resp) = match tokio_tungstenite::connect_async(&req.target).await {
            Ok(x) => x,
            Err(e) => {
                return RawProbe {
                    transport_ok: false,
                    latency_ms: t.elapsed().as_millis() as u64,
                    error: Some(e.to_string()),
                    ..Default::default()
                }
            }
        };

        let Some(payload) = req.payload.clone() else {
            let _ = ws.close(None).await;
            return RawProbe {
                transport_ok: true,
                status: Some(0),
                latency_ms: t.elapsed().as_millis() as u64,
                output: Some("connected".to_string()),
                error: None,
            };
        };

        if let Err(e) = ws.send(Message::Text(payload)).await {
            return RawProbe {
                transport_ok: false,
                latency_ms: t.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
                ..Default::default()
            };
        }

        let output = match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
            Ok(Some(Ok(msg))) => Some(message_to_string(&msg)),
            Ok(Some(Err(e))) => {
                return RawProbe {
                    transport_ok: false,
                    latency_ms: t.elapsed().as_millis() as u64,
                    error: Some(e.to_string()),
                    ..Default::default()
                }
            }
            Ok(None) => Some("<closed>".to_string()),
            Err(_) => {
                return RawProbe {
                    transport_ok: false,
                    latency_ms: t.elapsed().as_millis() as u64,
                    error: Some("recv timeout".to_string()),
                    ..Default::default()
                }
            }
        };
        let _ = ws.close(None).await;
        RawProbe {
            transport_ok: true,
            status: Some(0),
            latency_ms: t.elapsed().as_millis() as u64,
            output,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_messages() {
        assert_eq!(message_to_string(&Message::Text("hi".into())), "hi");
        assert_eq!(message_to_string(&Message::Binary(b"ab".to_vec().into())), "ab");
    }

    #[tokio::test]
    async fn unreachable_target_is_transport_failure() {
        let plugin = WebSocketPlugin::new();
        let req = ProbeRequest {
            protocol: "websocket".into(),
            target: "ws://127.0.0.1:1/".into(),
            payload: Some("ping".into()),
            metadata: Default::default(),
            assertions: vec![],
        };
        let raw = plugin.run(&req).await;
        assert!(!raw.transport_ok);
        assert!(raw.error.is_some());
    }
}
