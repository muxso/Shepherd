use super::{GrpcError, GrpcTarget};
use hyper_util::rt::TokioIo;
use std::{
    net::Shutdown,
    sync::{Arc, Mutex},
    time::Duration,
};
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};
use tower::service_fn;
struct ConnectionState {
    closed: bool,
    socket: Option<std::net::TcpStream>,
}
pub(super) struct OwnedConnection(Arc<Mutex<ConnectionState>>);
impl Drop for OwnedConnection {
    fn drop(&mut self) {
        let mut state = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closed = true;
        if let Some(socket) = state.socket.take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}
pub(super) async fn connect(
    target: &GrpcTarget<'_>,
    duration: Duration,
) -> Result<(Channel, OwnedConnection), GrpcError> {
    let uri = super::endpoint(target)?;
    let host = uri
        .host_str()
        .ok_or(GrpcError::InvalidTarget)?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    let port = uri.port_or_known_default().ok_or(GrpcError::InvalidTarget)?;
    let mut endpoint = Endpoint::from_shared(target.endpoint.to_string())?
        .connect_timeout(duration)
        .timeout(duration)
        .http2_max_header_list_size(65_536);
    if uri.scheme() == "https" {
        let mut tls = ClientTlsConfig::new();
        tls = match target.ca_certificate {
            Some(ca) => tls.ca_certificate(Certificate::from_pem(ca)),
            None => tls.with_webpki_roots(),
        };
        if let Some(name) = target.server_name {
            tls = tls.domain_name(name);
        }
        if let Some(identity) = &target.identity {
            tls = tls.identity(Identity::from_pem(identity.certificate, identity.private_key));
        }
        endpoint = endpoint.tls_config(tls)?;
    }
    let state = Arc::new(Mutex::new(ConnectionState { closed: false, socket: None }));
    let guard = OwnedConnection(state.clone());
    let channel = endpoint
        .connect_with_connector(service_fn(move |_: http::Uri| {
            let state = state.clone();
            let host = host.clone();
            async move {
                {
                    let state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    if state.closed || state.socket.is_some() {
                        return Err(std::io::Error::other(
                            "one-shot gRPC connection cannot reconnect",
                        ));
                    }
                }
                let stream =
                    tokio::net::TcpStream::connect((host.as_str(), port)).await?.into_std()?;
                {
                    let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    if state.closed || state.socket.is_some() {
                        let _ = stream.shutdown(Shutdown::Both);
                        return Err(std::io::Error::other("gRPC connection cancelled"));
                    }
                    state.socket = Some(stream.try_clone()?);
                }
                Ok::<_, std::io::Error>(TokioIo::new(tokio::net::TcpStream::from_std(stream)?))
            }
        }))
        .await?;
    Ok((channel, guard))
}
