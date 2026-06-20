//! WebSocket 协议插件:target=ws URL(ws://host:port/path),payload=要发送的文本(可空)。
//!
//! 有 payload:连接 → 发送 → 读一条回复(5s 超时)→ 关闭,output=回复文本。
//! 无 payload:仅验证能否握手连上,output="connected"。status=0(OK)/None(失败)。
//!
//! WebSocket 是有状态的双向流,不像 http/redis/grpc 那样可多路复用共享,故**不缓存连接**:
//! 每次探测/压测都走完整 连接→交互→关闭 生命周期(这也是真实 ws 客户端的行为)。
//! 纯 Rust(tokio-tungstenite,默认无 TLS);wss(TLS)留作后续。

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

/// 一条 WebSocket 消息转成可断言字符串。
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

        // 无 payload:仅验证握手成功。
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

        // 读一条回复(5s 超时,避免服务端不回时挂死)。
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
