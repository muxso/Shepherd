//! 每客户端令牌桶限流。opt-in:`SHEPHERD_RATE_LIMIT_RPS>0` 才启用。
//! 客户端 key 取 `X-Forwarded-For` / `X-Real-IP` 首段(经反代);取不到归一到 "unknown"。
//! 桶逻辑与时钟解耦(`check` 收外部 `Instant`),纯逻辑可测。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use axum::extract::{Request, State};
use axum::http::{header::RETRY_AFTER, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

struct Bucket {
    tokens: f64,
    last: Instant,
}

pub struct RateLimiter {
    rps: f64,
    burst: f64,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    pub fn from_env() -> Option<Arc<Self>> {
        let rps = std::env::var("SHEPHERD_RATE_LIMIT_RPS").ok()?.trim().parse::<f64>().ok()?;
        if rps <= 0.0 {
            return None;
        }
        let burst = std::env::var("SHEPHERD_RATE_LIMIT_BURST")
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|b| *b >= 1.0)
            .unwrap_or((rps * 2.0).max(1.0));
        Some(Arc::new(Self { rps, burst, buckets: Mutex::new(HashMap::new()) }))
    }

    /// 放行返回 Ok;拒绝返回建议等待秒(≥1)。
    fn check(&self, key: &str, now: Instant) -> Result<(), u64> {
        let mut g = self.buckets.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let b = g.entry(key.to_string()).or_insert(Bucket { tokens: self.burst, last: now });
        let elapsed = now.saturating_duration_since(b.last).as_secs_f64();
        b.tokens = (b.tokens + elapsed * self.rps).min(self.burst);
        b.last = now;
        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            Ok(())
        } else {
            Err((((1.0 - b.tokens) / self.rps).ceil() as u64).max(1))
        }
    }
}

fn client_key(h: &HeaderMap) -> String {
    h.get("x-forwarded-for")
        .or_else(|| h.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

pub async fn layer(State(rl): State<Arc<RateLimiter>>, req: Request, next: Next) -> Response {
    match rl.check(&client_key(req.headers()), Instant::now()) {
        Ok(()) => next.run(req).await,
        Err(wait) => {
            let mut resp = (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
            resp.headers_mut().insert(RETRY_AFTER, HeaderValue::from(wait));
            resp
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn limiter(rps: f64, burst: f64) -> RateLimiter {
        RateLimiter { rps, burst, buckets: Mutex::new(HashMap::new()) }
    }

    #[test]
    fn allows_burst_then_429_then_refills() {
        let rl = limiter(1.0, 2.0);
        let t0 = Instant::now();
        // burst=2 → 头两次放行。
        assert!(rl.check("ip", t0).is_ok());
        assert!(rl.check("ip", t0).is_ok());
        // 第三次(同一时刻)拒绝,建议等待 1s。
        assert_eq!(rl.check("ip", t0), Err(1));
        // 过 1s 回填 1 个令牌 → 放行一次。
        assert!(rl.check("ip", t0 + Duration::from_secs(1)).is_ok());
        assert!(rl.check("ip", t0 + Duration::from_secs(1)).is_err());
    }

    #[test]
    fn buckets_are_per_client() {
        let rl = limiter(1.0, 1.0);
        let t = Instant::now();
        assert!(rl.check("a", t).is_ok());
        assert!(rl.check("a", t).is_err());
        // 另一个 client 不受影响。
        assert!(rl.check("b", t).is_ok());
    }

    #[test]
    fn client_key_prefers_forwarded_for_first_hop() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", HeaderValue::from_static("1.2.3.4, 10.0.0.1"));
        assert_eq!(client_key(&h), "1.2.3.4");
        let mut h2 = HeaderMap::new();
        h2.insert("x-real-ip", HeaderValue::from_static("9.9.9.9"));
        assert_eq!(client_key(&h2), "9.9.9.9");
        assert_eq!(client_key(&HeaderMap::new()), "unknown");
    }
}
