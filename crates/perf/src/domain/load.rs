use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LoadError {
    #[error("concurrency must be > 0")]
    ZeroConcurrency,
    #[error("iterations must be > 0")]
    ZeroIterations,
    #[error("duration_ms must be > 0")]
    ZeroDuration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadMode {
    Iterations(usize),
    DurationMs(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadPlan {
    pub concurrency: usize,
    pub mode: LoadMode,
}

impl LoadPlan {
    pub fn new(concurrency: usize, iterations: usize) -> Result<Self, LoadError> {
        if concurrency == 0 {
            return Err(LoadError::ZeroConcurrency);
        }
        if iterations == 0 {
            return Err(LoadError::ZeroIterations);
        }
        Ok(Self { concurrency, mode: LoadMode::Iterations(iterations) })
    }

    pub fn duration_ms(concurrency: usize, duration_ms: u64) -> Result<Self, LoadError> {
        if concurrency == 0 {
            return Err(LoadError::ZeroConcurrency);
        }
        if duration_ms == 0 {
            return Err(LoadError::ZeroDuration);
        }
        Ok(Self { concurrency, mode: LoadMode::DurationMs(duration_ms) })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    pub latency_ms: u64,
    pub success: bool,
}

impl Sample {
    pub fn new(latency_ms: u64, success: bool) -> Self {
        Self { latency_ms, success }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyStats {
    pub min: u64,
    pub mean: u64,
    pub p50: u64,
    pub p90: u64,
    pub p95: u64,
    pub p99: u64,
    pub max: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadReport {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub error_rate: f64,
    pub elapsed_ms: u64,
    pub throughput_rps: f64,
    pub latency: LatencyStats,
}

// nearest-rank 分位:取 ceil(p/100*n) 名(1 基),非插值。
fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let n = sorted.len();
    let rank = ((p / 100.0) * n as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(n - 1);
    sorted[idx]
}

// 延迟分位对全部样本计算(含失败):失败多为慢/超时,纳入才反映真实尾延迟。
pub fn aggregate(samples: &[Sample], elapsed_ms: u64) -> LoadReport {
    let total = samples.len();
    let success = samples.iter().filter(|s| s.success).count();
    let failed = total - success;
    let error_rate = if total == 0 { 0.0 } else { failed as f64 / total as f64 };
    let throughput_rps =
        if elapsed_ms == 0 { 0.0 } else { total as f64 / (elapsed_ms as f64 / 1000.0) };

    let mut lat: Vec<u64> = samples.iter().map(|s| s.latency_ms).collect();
    lat.sort_unstable();
    let latency = if lat.is_empty() {
        LatencyStats { min: 0, mean: 0, p50: 0, p90: 0, p95: 0, p99: 0, max: 0 }
    } else {
        let sum: u128 = lat.iter().map(|v| *v as u128).sum();
        let mean = (sum as f64 / lat.len() as f64).round() as u64;
        LatencyStats {
            min: lat[0],
            mean,
            p50: percentile(&lat, 50.0),
            p90: percentile(&lat, 90.0),
            p95: percentile(&lat, 95.0),
            p99: percentile(&lat, 99.0),
            max: lat[lat.len() - 1],
        }
    };

    LoadReport { total, success, failed, error_rate, elapsed_ms, throughput_rps, latency }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_validates_nonzero() {
        assert_eq!(LoadPlan::new(0, 10).unwrap_err(), LoadError::ZeroConcurrency);
        assert_eq!(LoadPlan::new(4, 0).unwrap_err(), LoadError::ZeroIterations);
        assert_eq!(LoadPlan::new(4, 100).expect("ok").mode, LoadMode::Iterations(100));
    }

    #[test]
    fn duration_plan_validates_and_sets_mode() {
        assert_eq!(LoadPlan::duration_ms(0, 1000).unwrap_err(), LoadError::ZeroConcurrency);
        assert_eq!(LoadPlan::duration_ms(4, 0).unwrap_err(), LoadError::ZeroDuration);
        assert_eq!(LoadPlan::duration_ms(4, 5000).expect("ok").mode, LoadMode::DurationMs(5000));
    }

    #[test]
    fn percentile_nearest_rank() {
        let v: Vec<u64> = (1..=10).collect();
        assert_eq!(percentile(&v, 50.0), 5);
        assert_eq!(percentile(&v, 90.0), 9);
        assert_eq!(percentile(&v, 99.0), 10);
        assert_eq!(percentile(&v, 100.0), 10);
        assert_eq!(percentile(&[], 95.0), 0);
        assert_eq!(percentile(&[7], 50.0), 7);
    }

    #[test]
    fn aggregate_counts_rate_and_throughput() {
        let samples = vec![
            Sample::new(10, true),
            Sample::new(20, true),
            Sample::new(30, false),
            Sample::new(40, true),
        ];
        let r = aggregate(&samples, 1000);
        assert_eq!(r.total, 4);
        assert_eq!(r.success, 3);
        assert_eq!(r.failed, 1);
        assert!((r.error_rate - 0.25).abs() < 1e-9);
        assert!((r.throughput_rps - 4.0).abs() < 1e-9);
        assert_eq!(r.latency.min, 10);
        assert_eq!(r.latency.max, 40);
        assert_eq!(r.latency.mean, 25);
    }

    #[test]
    fn aggregate_latency_includes_failures() {
        let samples = vec![Sample::new(5, true), Sample::new(5, true), Sample::new(900, false)];
        let r = aggregate(&samples, 1000);
        assert_eq!(r.latency.max, 900);
        assert_eq!(r.latency.p99, 900);
    }

    #[test]
    fn aggregate_empty_is_all_zero() {
        let r = aggregate(&[], 0);
        assert_eq!(r.total, 0);
        assert_eq!(r.error_rate, 0.0);
        assert_eq!(r.throughput_rps, 0.0);
        assert_eq!(r.latency.p95, 0);
    }

    #[test]
    fn report_serializes_camel_case() {
        let r = aggregate(&[Sample::new(10, true)], 1000);
        let j = serde_json::to_value(r).expect("json");
        assert!(j.get("errorRate").is_some());
        assert!(j.get("throughputRps").is_some());
        assert!(j["latency"].get("p95").is_some());
    }
}
