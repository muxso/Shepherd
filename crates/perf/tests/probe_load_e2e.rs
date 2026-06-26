#![cfg(all(feature = "engine", feature = "probe-exec"))]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::{routing::get, Router};
use perf::adapters::{run_load, ProbeExecutor};
use perf::domain::LoadPlan;
use probe::{default_registry, ProbeAssertion, ProbeRequest};
use tokio::net::TcpListener;

async fn spawn_server() -> String {
    let hits = Arc::new(AtomicUsize::new(0));
    let app = Router::new().route(
        "/ping",
        get({
            let hits = hits.clone();
            move || {
                let hits = hits.clone();
                async move {
                    let n = hits.fetch_add(1, Ordering::SeqCst) + 1;
                    if n % 10 == 0 {
                        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "boom")
                    } else {
                        (axum::http::StatusCode::OK, "pong")
                    }
                }
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{addr}")
}

#[tokio::test]
async fn perf_through_probe_registry_produces_report() {
    let base = spawn_server().await;
    let req = ProbeRequest {
        protocol: "http".to_string(),
        target: format!("{base}/ping"),
        payload: None,
        metadata: Default::default(),
        assertions: vec![ProbeAssertion::StatusIs(200)],
    };
    let registry = Arc::new(default_registry());
    let exec = Arc::new(ProbeExecutor::new(registry, req));
    let plan = LoadPlan::new(5, 30).expect("plan");

    let report = run_load(&plan, exec).await;

    assert_eq!(report.total, 30);
    assert_eq!(report.success + report.failed, 30);
    assert!(report.failed >= 1, "每 10 次一个 500,断言应判失败: {report:?}");
    assert!(report.throughput_rps > 0.0);
    assert!(report.latency.p50 <= report.latency.p95);
    assert!(report.latency.p95 <= report.latency.max);
}
