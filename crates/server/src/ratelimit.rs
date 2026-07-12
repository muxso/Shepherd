//! Per-client token-bucket rate limiting. Enabled by default (200 rps/client);
//! `SHEPHERD_RATE_LIMIT_RPS=0` disables it explicitly.
//! Client key is the first hop of `X-Forwarded-For` / `X-Real-IP` (behind a reverse
//! proxy); without one it collapses to "unknown" — i.e. direct connections all share
//! one bucket, so the default must be generous. Real login brute-force protection is
//! handled by the lockout mechanism.
//! Bucket logic is decoupled from the clock (`check` takes an external `Instant`),
//! keeping it testable as pure logic.

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

const DEFAULT_RPS: f64 = 200.0;

impl RateLimiter {
    pub fn from_env() -> Option<Arc<Self>> {
        let rps = match std::env::var("SHEPHERD_RATE_LIMIT_RPS") {
            Ok(v) => v.trim().parse::<f64>().ok()?,
            Err(_) => DEFAULT_RPS,
        };
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

    /// Ok = allowed; Err = rejected with the suggested wait in seconds (>= 1).
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
        // burst=2 -> first two pass.
        assert!(rl.check("ip", t0).is_ok());
        assert!(rl.check("ip", t0).is_ok());
        // Third (same instant) rejected with a 1s suggested wait.
        assert_eq!(rl.check("ip", t0), Err(1));
        // After 1s one token refills -> one more pass.
        assert!(rl.check("ip", t0 + Duration::from_secs(1)).is_ok());
        assert!(rl.check("ip", t0 + Duration::from_secs(1)).is_err());
    }

    #[test]
    fn buckets_are_per_client() {
        let rl = limiter(1.0, 1.0);
        let t = Instant::now();
        assert!(rl.check("a", t).is_ok());
        assert!(rl.check("a", t).is_err());
        // A different client is unaffected.
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
