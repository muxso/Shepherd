//! gRPC 协议执行器:对目标 gRPC 服务反复调用一个 unary 方法并计时(协议广度,tonic)。
//!
//! 用字节透传 codec(不依赖 .proto/prost):按全方法路径(如 `/grpc.health.v1.Health/Check`)
//! 发送预编码请求体,成功 = gRPC 状态 OK。与 HTTP/SQL 并列实现 `RequestExecutor`,
//! 故压测引擎(并发/计时/分位/样本下沉)对 gRPC 完全复用。channel 在并发 worker 间共享(HTTP/2 多路复用)。

use async_trait::async_trait;
use bytes::{Buf, BufMut, Bytes};
use http::uri::PathAndQuery;
use tonic::client::Grpc;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::transport::Channel;
use tonic::{Request, Status};

use crate::ports::RequestExecutor;

/// 字节透传 codec:请求/响应都按原始字节处理,可调任意 unary 方法而无需 proto。
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

pub struct GrpcExecutor {
    channel: Channel,
    method: PathAndQuery,
    payload: Bytes,
}

impl GrpcExecutor {
    /// 连接 gRPC 端点(如 http://host:50051),预置全方法路径与请求体。建连即时失败快速返回。
    pub async fn connect(
        endpoint: &str,
        method: &str,
        payload: Vec<u8>,
    ) -> Result<Self, String> {
        let channel = Channel::from_shared(endpoint.to_string())
            .map_err(|e| e.to_string())?
            .connect()
            .await
            .map_err(|e| e.to_string())?;
        let method = PathAndQuery::try_from(method).map_err(|e| e.to_string())?;
        Ok(Self { channel, method, payload: Bytes::from(payload) })
    }
}

#[async_trait]
impl RequestExecutor for GrpcExecutor {
    async fn execute(&self) -> bool {
        let mut grpc = Grpc::new(self.channel.clone());
        if grpc.ready().await.is_err() {
            return false;
        }
        let req = Request::new(self.payload.clone());
        grpc.unary(req, self.method.clone(), BytesCodec).await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_to_unreachable_endpoint_errors() {
        // 不可达端点 → connect 即报错(快速失败,不阻塞)。
        let r = GrpcExecutor::connect("http://127.0.0.1:1", "/svc/M", vec![]).await;
        assert!(r.is_err());
    }

    /// 真 gRPC 压测:对一个 tonic-health 服务压 Health/Check。需起本地服务,故 #[ignore]。
    #[tokio::test]
    #[ignore = "需要本地 gRPC 服务"]
    async fn grpc_health_load() {
        use crate::adapters::run_load;
        use crate::domain::LoadPlan;
        use std::sync::Arc;
        use tokio::net::TcpListener;

        let (_reporter, health_svc) = tonic_health::server::health_reporter();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(health_svc)
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
                .expect("serve");
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // 空 HealthCheckRequest(默认 proto)= 空字节。
        let exec = Arc::new(
            GrpcExecutor::connect(&format!("http://{addr}"), "/grpc.health.v1.Health/Check", vec![])
                .await
                .expect("connect"),
        );
        let report = run_load(&LoadPlan::new(4, 40).expect("plan"), exec).await;
        assert_eq!(report.total, 40);
        assert_eq!(report.failed, 0);
    }
}
