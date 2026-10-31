use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
async fn echo_authority<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    stream: &mut T,
    sni: &str,
) {
    let mut request = Vec::new();
    let mut buffer = [0; 1024];
    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        let n = stream.read(&mut buffer).await.expect("read request");
        assert!(n > 0 && request.len() + n < 65_536);
        request.extend_from_slice(&buffer[..n]);
    }
    let request = String::from_utf8(request).expect("headers");
    let host = request
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("host"))
                .map(|(_, value)| value.trim())
        })
        .expect("authority");
    let body = format!("host={host};sni={sni}");
    stream
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .expect("response");
    stream.shutdown().await.expect("shutdown");
}
fn rules() -> HostOverrides {
    HostOverrides::new(
        [
            ("example.test".into(), vec!["127.0.0.1".parse().expect("IP")]),
            ("wrong.test".into(), vec!["127.0.0.1".parse().expect("IP")]),
        ]
        .into(),
    )
    .expect("overrides")
}
fn request(url: String) -> RequestSpec {
    RequestSpec { method: crate::HttpMethod::Get, url, headers: vec![], body: None }
}
#[tokio::test]
async fn host_overrides_preserve_http_authority_and_do_not_leak_between_runners() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        echo_authority(&mut stream, "").await;
    });
    let runner = ReqwestRunner::with_host_overrides(&rules()).expect("runner");
    let spec = request(format!("http://example.test:{port}/test?value=1"));
    let response = runner.execute(&spec).await.expect("overridden HTTP");
    assert_eq!(response.body, format!("host=example.test:{port};sni="));
    assert_eq!(response.timings.dns_ms, Some(0));
    server.await.expect("server");
    let isolated = ReqwestRunner::no_proxy();
    assert!(isolated.overridden_hosts.is_empty());
    assert!(isolated.execute(&spec).await.is_err(), "override must not become process-global");
}
#[tokio::test]
async fn host_overrides_preserve_tls_sni_and_still_verify_the_original_name() {
    use tokio_rustls::{
        rustls::{
            pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
            ServerConfig,
        },
        TlsAcceptor,
    };
    let key = rcgen::KeyPair::generate().expect("key");
    let cert = rcgen::CertificateParams::new(vec!["example.test".into()])
        .expect("parameters")
        .self_signed(&key)
        .expect("certificate");
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert.der().to_vec())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
        )
        .expect("TLS config");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let acceptor = TlsAcceptor::from(std::sync::Arc::new(config));
        for _ in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept");
            if let Ok(mut stream) = acceptor.accept(stream).await {
                let sni = stream.get_ref().1.server_name().expect("SNI").to_string();
                echo_authority(&mut stream, &sni).await;
            }
        }
    });
    let builder = configured_builder().add_root_certificate(
        reqwest::Certificate::from_pem(cert.pem().as_bytes()).expect("trusted fixture"),
    );
    let runner = ReqwestRunner::build_overridden(builder, &rules()).expect("runner");
    let response = runner
        .execute(&request(format!("https://example.test:{port}/")))
        .await
        .expect("verified HTTPS");
    assert_eq!(response.body, format!("host=example.test:{port};sni=example.test"));
    assert!(
        runner.execute(&request(format!("https://wrong.test:{port}/"))).await.is_err(),
        "resolver overrides never bypass name verification"
    );
    server.await.expect("server");
}
