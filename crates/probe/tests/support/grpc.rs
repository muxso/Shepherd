//! Native loopback protobuf/TLS fixture: no external server or protoc.
use rcgen::{BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tonic::{
    codegen::{http, Body, BoxFuture, Service, StdError},
    server::{NamedService, UnaryService},
    transport::{Certificate, Identity, Server, ServerTlsConfig},
    Request, Response, Status,
};

pub const PROTO: &str = r#"syntax="proto3";package fixture;import "google/protobuf/timestamp.proto";
message Message {string value=1;int64 count=2;google.protobuf.Timestamp time=3;bytes data=4;}
service Echo {rpc Call(Message) returns(Message);rpc Stream(stream Message) returns(stream Message);}"#;
#[derive(Clone, PartialEq, prost::Message)]
struct Message {
    #[prost(string, tag = "1")]
    value: String,
    #[prost(int64, tag = "2")]
    count: i64,
    #[prost(message, optional, tag = "3")]
    time: Option<prost_types::Timestamp>,
    #[prost(bytes = "vec", tag = "4")]
    data: Vec<u8>,
}
#[derive(Clone)]
struct Echo(Arc<AtomicUsize>);
struct Call(Arc<AtomicUsize>);
impl UnaryService<Message> for Call {
    type Response = Message;
    type Future = BoxFuture<Response<Message>, Status>;
    fn call(&mut self, request: Request<Message>) -> Self::Future {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if request.metadata().get("authorization").and_then(|v| v.to_str().ok())
                != Some("Bearer fixture-token")
            {
                return Err(Status::unauthenticated("missing credential"));
            }
            let mut message = request.into_inner();
            match message.value.as_str() {
                "fail" => return Err(Status::permission_denied("fixture rejection")),
                "block" => tokio::time::sleep(std::time::Duration::from_secs(5)).await,
                "large" => message.value = "x".repeat(100_000),
                _ => {}
            }
            let mut response = Response::new(message);
            response.metadata_mut().insert("x-result", "fixture".parse().expect("metadata"));
            response
                .metadata_mut()
                .insert_bin("x-bytes-bin", tonic::metadata::MetadataValue::from_bytes(&[0, 255]));
            Ok(response)
        })
    }
}
impl<B: Body + Send + 'static> Service<http::Request<B>> for Echo
where
    B::Error: Into<StdError> + Send + 'static,
{
    type Response = http::Response<tonic::body::BoxBody>;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;
    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn call(&mut self, request: http::Request<B>) -> Self::Future {
        let calls = self.0.clone();
        Box::pin(async move {
            let mut grpc = tonic::server::Grpc::new(tonic::codec::ProstCodec::default());
            Ok(grpc.unary(Call(calls), request).await)
        })
    }
}
impl NamedService for Echo {
    const NAME: &'static str = "fixture.Echo";
}
struct Connection {
    socket: tokio::net::TcpStream,
    active: Arc<AtomicUsize>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }
}
impl tonic::transport::server::Connected for Connection {
    type ConnectInfo = tonic::transport::server::TcpConnectInfo;
    fn connect_info(&self) -> Self::ConnectInfo {
        self.socket.connect_info()
    }
}
impl tokio::io::AsyncRead for Connection {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.socket).poll_read(cx, buf)
    }
}
impl tokio::io::AsyncWrite for Connection {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.socket).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.socket).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.socket).poll_shutdown(cx)
    }
}
pub struct Fixture {
    pub endpoint: String,
    pub ca: String,
    pub certificate: String,
    pub key: String,
    pub calls: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Fixture {
    pub async fn idle(&self) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while self.active.load(Ordering::SeqCst) != 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("gRPC sockets close on completion/error/cancellation");
    }
    pub fn files() -> BTreeMap<String, String> {
        [("fixture.proto".into(), PROTO.into())].into()
    }
    pub async fn start(tls: bool) -> Self {
        let mut params = CertificateParams::new(Vec::<String>::new()).expect("CA params");
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca_key = KeyPair::generate().expect("CA key");
        let ca = params.self_signed(&ca_key).expect("CA");
        let server_key = KeyPair::generate().expect("server key");
        let mut params = CertificateParams::new(vec!["localhost".into()]).expect("server params");
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let server = params.signed_by(&server_key, &ca, &ca_key).expect("server certificate");
        let client_key = KeyPair::generate().expect("client key");
        let mut params =
            CertificateParams::new(vec!["fixture-client".into()]).expect("client params");
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        let client = params.signed_by(&client_key, &ca, &ca_key).expect("client certificate");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let endpoint = format!(
            "{}://{}",
            if tls { "https" } else { "http" },
            listener.local_addr().expect("address")
        );
        let calls = Arc::new(AtomicUsize::new(0));
        let mut builder = Server::builder();
        if tls {
            builder = builder
                .tls_config(
                    ServerTlsConfig::new()
                        .identity(Identity::from_pem(server.pem(), server_key.serialize_pem()))
                        .client_ca_root(Certificate::from_pem(ca.pem())),
                )
                .expect("TLS");
        }
        let service = Echo(calls.clone());
        let active = Arc::new(AtomicUsize::new(0));
        let count = active.clone();
        use tokio_stream::StreamExt;
        let incoming =
            tokio_stream::wrappers::TcpListenerStream::new(listener).map(move |socket| {
                socket.map(|socket| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Connection { socket, active: count.clone() }
                })
            });
        let task = tokio::spawn(async move {
            builder.add_service(service).serve_with_incoming(incoming).await.expect("server");
        });
        Self {
            endpoint,
            ca: ca.pem(),
            certificate: client.pem(),
            key: client_key.serialize_pem(),
            calls,
            active,
            task,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
