#![cfg(feature = "websocket")]

use futures_util::{SinkExt, StreamExt};
use probe::{default_registry, ProbeAssertion, ProbeRequest};
use tokio::net::TcpListener;

async fn spawn_echo() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut ws = match tokio_tungstenite::accept_async(stream).await {
                    Ok(ws) => ws,
                    Err(_) => return,
                };
                while let Some(Ok(msg)) = ws.next().await {
                    if msg.is_close() {
                        break;
                    }
                    if msg.is_text() || msg.is_binary() {
                        let _ = ws.send(msg).await;
                    }
                }
            });
        }
    });
    format!("ws://{addr}")
}

#[tokio::test]
async fn ws_probe_echo_through_registry() {
    let base = spawn_echo().await;
    let reg = default_registry();
    assert!(reg.protocols().contains(&"websocket".to_string()), "websocket 插件应在注册表");

    let out = reg
        .dispatch(&ProbeRequest {
            protocol: "websocket".into(),
            target: base.clone(),
            payload: Some("hello probe".into()),
            metadata: Default::default(),
            assertions: vec![
                ProbeAssertion::Success,
                ProbeAssertion::OutputEquals("hello probe".into()),
            ],
        })
        .await;
    assert!(out.success, "echo 应回原文且断言通过: {out:?}");
    assert_eq!(out.output.as_deref(), Some("hello probe"));

    let out = reg
        .dispatch(&ProbeRequest {
            protocol: "websocket".into(),
            target: base,
            payload: None,
            metadata: Default::default(),
            assertions: vec![ProbeAssertion::Success],
        })
        .await;
    assert!(out.success, "握手应成功: {out:?}");
    assert_eq!(out.output.as_deref(), Some("connected"));
}
