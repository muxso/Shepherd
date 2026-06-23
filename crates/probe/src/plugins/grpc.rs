//! gRPC 协议插件:target=端点,metadata["method"]=全方法路径,payload=请求字节(可空)。
//! 字节透传 codec(不依赖 .proto);status=0(OK),output=响应字节长度摘要。
//!
//! 按端点缓存 Channel:压测时复用同一 HTTP/2 连接(Channel 克隆共享底层连接),
//! 不会每请求重连,测的是目标服务吞吐而非建连开销。

use async_trait::async_trait;
use bytes::{Buf, BufMut, Bytes};
use http::uri::PathAndQuery;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use tonic::client::Grpc;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::transport::Channel;
use tonic::{Request, Status};

use crate::domain::{ProbeRequest, RawProbe};
use crate::ports::ProtocolPlugin;

#[derive(Default)]
struct BytesCodec;
struct BytesEncoder;
impl Encoder for BytesEncoder {
    type Item = Bytes;
    type Error = Status;
    fn encode(&mut self, item: Bytes, dst: &mut EncodeBuf<'_>) -> Result<(), Status> {
        dst.put(item);
        Ok(())
    }
}
struct BytesDecoder;
impl Decoder for BytesDecoder {
    type Item = Bytes;
    type Error = Status;
    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Bytes>, Status> {
        let n = src.remaining();
        Ok(Some(src.copy_to_bytes(n)))
    }
}
impl Codec for BytesCodec {
    type Encode = Bytes;
    type Decode = Bytes;
    type Encoder = BytesEncoder;
    type Decoder = BytesDecoder;
    fn encoder(&mut self) -> Self::Encoder {
        BytesEncoder
    }
    fn decoder(&mut self) -> Self::Decoder {
        BytesDecoder
    }
}

#[derive(Default)]
pub struct GrpcPlugin {
    /// 端点 → Channel(克隆复用同一 HTTP/2 连接)。
    channels: Mutex<HashMap<String, Channel>>,
}

impl GrpcPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// 取目标端点的 Channel(已有则复用)。连接在锁外建立。
    async fn channel_for(&self, target: &str) -> Result<Channel, String> {
        if let Some(c) = self.channels.lock().expect("channels lock").get(target).cloned() {
            return Ok(c);
        }
        let endpoint = Channel::from_shared(target.to_string()).map_err(|e| e.to_string())?;
        let channel = endpoint.connect().await.map_err(|e| e.to_string())?;
        Ok(self
            .channels
            .lock()
            .expect("channels lock")
            .entry(target.to_string())
            .or_insert(channel)
            .clone())
    }
}

#[async_trait]
impl ProtocolPlugin for GrpcPlugin {
    fn protocol(&self) -> &'static str {
        "grpc"
    }

    async fn run(&self, req: &ProbeRequest) -> RawProbe {
        let method = match req.metadata.get("method") {
            // gRPC 路径必须以 / 开头;调用方常写 pkg.Service/Method,这里宽容补全。
            Some(m) if m.starts_with('/') => m.clone(),
            Some(m) if !m.trim().is_empty() => format!("/{}", m.trim()),
            _ => {
                return RawProbe {
                    transport_ok: false,
                    error: Some("grpc requires metadata.method (full path)".into()),
                    ..Default::default()
                }
            }
        };
        let path = match PathAndQuery::try_from(method) {
            Ok(p) => p,
            Err(e) => {
                return RawProbe { transport_ok: false, error: Some(e.to_string()), ..Default::default() }
            }
        };
        let channel = match self.channel_for(&req.target).await {
            Ok(ch) => ch,
            Err(e) => {
                return RawProbe { transport_ok: false, error: Some(e), ..Default::default() }
            }
        };
        let payload = Bytes::from(req.payload.clone().unwrap_or_default().into_bytes());
        let mut grpc = Grpc::new(channel);
        let t = Instant::now();
        if let Err(e) = grpc.ready().await {
            return RawProbe {
                transport_ok: false,
                latency_ms: t.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
                ..Default::default()
            };
        }
        match grpc.unary(Request::new(payload), path, BytesCodec).await {
            Ok(resp) => RawProbe {
                transport_ok: true,
                status: Some(0),
                latency_ms: t.elapsed().as_millis() as u64,
                output: Some(format!("{} bytes", resp.into_inner().len())),
                error: None,
            },
            Err(s) => RawProbe {
                transport_ok: false,
                status: Some(s.code() as i64),
                latency_ms: t.elapsed().as_millis() as u64,
                error: Some(s.message().to_string()),
                ..Default::default()
            },
        }
    }
}
