//! 端到端:用原生 reqwest 执行器对一个本地 axum 服务跑一轮压测,产出真实报告。
//! 需要 `engine` + `api-runner-exec` feature。
#![cfg(all(feature = "engine", feature = "api-runner-exec"))]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use api_runner::{Assertion, HttpMethod, RequestSpec};
use axum::{routing::get, Router};
use perf::adapters::{run_load, ApiRunnerExecutor};
use perf::domain::LoadPlan;
use tokio::net::TcpListener;

async fn spawn_server() -> String {
    // 计数器:前若干次返回 500,其余 200 —— 验证报告能区分成败。
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
async fn native_load_run_produces_report() {
    let base = spawn_server().await;
    let spec = RequestSpec {
        method: HttpMethod::Get,
        url: format!("{base}/ping"),
        headers: vec![],
        body: None,
    };
    // 成功 = HTTP 200(StatusIs(200) 断言)。
    let exec = Arc::new(ApiRunnerExecutor::new(spec, vec![Assertion::StatusIs(200)]));
    let plan = LoadPlan::new(5, 30).expect("plan");

    let report = run_load(&plan, exec).await;

    assert_eq!(report.total, 30);
    assert_eq!(report.success + report.failed, 30);
    assert!(report.failed >= 1, "每 10 次一个 500,应有失败: {report:?}");
    assert!(report.throughput_rps > 0.0);
    // 延迟分位应是合理的非降序。
    assert!(report.latency.p50 <= report.latency.p95);
    assert!(report.latency.p95 <= report.latency.max);
}
