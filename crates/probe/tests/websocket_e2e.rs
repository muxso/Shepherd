//! 端到端:测试内起一个本地 WebSocket echo 服务,经注册表对它探测 —— 验证
//! ws 插件 连接→发送→收回→断言 这条链完整可用、断言生效。需要 `websocket` feature。
#![cfg(feature = "websocket")]

use futures_util::{SinkExt, StreamExt};
use probe::{default_registry, ProbeAssertion, ProbeRequest};
use tokio::net::TcpListener;

/// 起一个把收到的文本/二进制原样回发的 ws echo 服务,返回其 ws:// 地址。
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

    // 发 "hello probe" → echo 回 同样内容,断言命中。
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

    // 无 payload:仅验证握手成功(output=connected)。
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
