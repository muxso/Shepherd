//! 进程内 Prometheus 指标:HTTP 请求计数 + 时延直方图(按 method/status),`/metrics` 暴露。
//! 零外部依赖;label 只取 method+status(不含原始 path,避免基数爆炸)。OTel 分布式追踪后续。

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::extract::{Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

const BUCKETS_MS: &[u64] = &[5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000];
const NB: usize = BUCKETS_MS.len();

#[derive(Default)]
struct Series {
    count: AtomicU64,
    sum_ms: AtomicU64,
    buckets: [AtomicU64; NB],
}

impl Series {
    fn observe(&self, ms: u64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum_ms.fetch_add(ms, Ordering::Relaxed);
        // 超过最大桶的只计入 +Inf(= count),不落任何显式桶。
        if let Some(i) = BUCKETS_MS.iter().position(|&b| ms <= b) {
            self.buckets[i].fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Default)]
pub struct Metrics {
    http: Mutex<HashMap<(String, u16), Arc<Series>>>,
}

impl Metrics {
    pub fn observe_http(&self, method: &str, status: u16, ms: u64) {
        let series = {
            let mut g = self.http.lock().expect("lock");
            g.entry((method.to_string(), status)).or_default().clone()
        };
        series.observe(ms);
    }

    pub fn render(&self) -> String {
        let snapshot: Vec<((String, u16), Arc<Series>)> = {
            let g = self.http.lock().expect("lock");
            g.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };
        let mut out = String::new();
        out.push_str("# HELP shepherd_http_requests_total Total HTTP requests.\n");
        out.push_str("# TYPE shepherd_http_requests_total counter\n");
        for ((method, status), s) in &snapshot {
            let _ = writeln!(
                out,
                "shepherd_http_requests_total{{method=\"{method}\",status=\"{status}\"}} {}",
                s.count.load(Ordering::Relaxed)
            );
        }
        out.push_str("# HELP shepherd_http_request_duration_ms Request latency (ms).\n");
        out.push_str("# TYPE shepherd_http_request_duration_ms histogram\n");
        for ((method, status), s) in &snapshot {
            let mut cumulative = 0u64;
            for (i, &b) in BUCKETS_MS.iter().enumerate() {
                cumulative += s.buckets[i].load(Ordering::Relaxed);
                let _ = writeln!(
                    out,
                    "shepherd_http_request_duration_ms_bucket{{method=\"{method}\",status=\"{status}\",le=\"{b}\"}} {cumulative}"
                );
            }
            let count = s.count.load(Ordering::Relaxed);
            let _ = writeln!(
                out,
                "shepherd_http_request_duration_ms_bucket{{method=\"{method}\",status=\"{status}\",le=\"+Inf\"}} {count}"
            );
            let _ = writeln!(
                out,
                "shepherd_http_request_duration_ms_sum{{method=\"{method}\",status=\"{status}\"}} {}",
                s.sum_ms.load(Ordering::Relaxed)
            );
            let _ = writeln!(
                out,
                "shepherd_http_request_duration_ms_count{{method=\"{method}\",status=\"{status}\"}} {count}"
            );
        }
        out
    }
}

/// 中间件:记录每个请求的 method/status/时延。挂在最外层以覆盖超时/限流转换后的最终状态。
pub async fn track(State(m): State<Arc<Metrics>>, req: Request, next: Next) -> Response {
    let method = req.method().as_str().to_owned();
    let start = Instant::now();
    let resp = next.run(req).await;
    m.observe_http(&method, resp.status().as_u16(), start.elapsed().as_millis() as u64);
    resp
}

pub async fn handler(State(m): State<Arc<Metrics>>) -> impl IntoResponse {
    ([(CONTENT_TYPE, "text/plain; version=0.0.4")], m.render())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_prometheus_counter_and_histogram() {
        let m = Metrics::default();
        m.observe_http("GET", 200, 3);
        m.observe_http("GET", 200, 7);
        m.observe_http("POST", 500, 600);
        let out = m.render();

        assert!(out.contains("shepherd_http_requests_total{method=\"GET\",status=\"200\"} 2"));
        assert!(out.contains("shepherd_http_requests_total{method=\"POST\",status=\"500\"} 1"));
        // 3ms ≤ le=5(累计 1);3ms,7ms ≤ le=10(累计 2)。
        assert!(out.contains("method=\"GET\",status=\"200\",le=\"5\"} 1"));
        assert!(out.contains("method=\"GET\",status=\"200\",le=\"10\"} 2"));
        assert!(out.contains("shepherd_http_request_duration_ms_sum{method=\"GET\",status=\"200\"} 10"));
        assert!(out.contains("shepherd_http_request_duration_ms_count{method=\"GET\",status=\"200\"} 2"));
        // 600ms 落入 le=1000;+Inf 与 count 都为 1。
        assert!(out.contains("method=\"POST\",status=\"500\",le=\"1000\"} 1"));
        assert!(out.contains("method=\"POST\",status=\"500\",le=\"+Inf\"} 1"));
    }
}
