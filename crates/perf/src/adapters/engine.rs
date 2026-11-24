use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::domain::{aggregate, LoadMode, LoadPlan, LoadReport, Sample};
use crate::ports::RequestExecutor;

pub async fn run_load(plan: &LoadPlan, exec: Arc<dyn RequestExecutor>) -> LoadReport {
    run_collect(plan, exec).await.0
}

pub async fn run_collect(
    plan: &LoadPlan,
    exec: Arc<dyn RequestExecutor>,
) -> (LoadReport, Vec<Sample>) {
    let start = Instant::now();
    let mut handles = Vec::with_capacity(plan.concurrency);
    for i in 0..plan.concurrency {
        let exec = exec.clone();
        let mode = plan.mode;
        let worker_iters = match mode {
            // First (n % concurrency) workers take one extra iteration each, summing exactly to n.
            LoadMode::Iterations(n) => n / plan.concurrency + usize::from(i < n % plan.concurrency),
            LoadMode::DurationMs(_) => 0,
        };
        handles.push(tokio::spawn(async move {
            let mut local = Vec::new();
            match mode {
                LoadMode::Iterations(_) => {
                    for _ in 0..worker_iters {
                        local.push(sample_once(&exec).await);
                    }
                }
                LoadMode::DurationMs(ms) => {
                    let deadline = Instant::now() + Duration::from_millis(ms);
                    while Instant::now() < deadline {
                        local.push(sample_once(&exec).await);
                    }
                }
            }
            local
        }));
    }

    let mut samples = Vec::new();
    for h in handles {
        if let Ok(mut local) = h.await {
            samples.append(&mut local);
        }
    }
    let elapsed_ms = start.elapsed().as_millis() as u64;
    (aggregate(&samples, elapsed_ms), samples)
}

async fn sample_once(exec: &Arc<dyn RequestExecutor>) -> Sample {
    let t = Instant::now();
    let success = exec.execute().await;
    Sample::new(t.elapsed().as_millis() as u64, success)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct EveryNthFails {
        calls: AtomicUsize,
        n: usize,
    }
    #[async_trait]
    impl RequestExecutor for EveryNthFails {
        async fn execute(&self) -> bool {
            let i = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            i % self.n != 0
        }
    }

    #[tokio::test]
    async fn runs_exact_iterations_and_aggregates() {
        let plan = LoadPlan::new(4, 20).expect("plan");
        let exec = Arc::new(EveryNthFails { calls: AtomicUsize::new(0), n: 5 });
        let report = run_load(&plan, exec).await;
        assert_eq!(report.total, 20);
        assert_eq!(report.failed, 4);
        assert_eq!(report.success, 16);
        assert!((report.error_rate - 0.2).abs() < 1e-9);
        assert!(report.throughput_rps >= 0.0);
    }

    #[tokio::test]
    async fn uneven_split_still_runs_all() {
        let plan = LoadPlan::new(4, 7).expect("plan");
        let exec = Arc::new(EveryNthFails { calls: AtomicUsize::new(0), n: 100 });
        let report = run_load(&plan, exec).await;
        assert_eq!(report.total, 7);
        assert_eq!(report.failed, 0);
    }

    #[tokio::test]
    async fn concurrency_exceeding_iterations_is_fine() {
        let plan = LoadPlan::new(8, 3).expect("plan");
        let exec = Arc::new(EveryNthFails { calls: AtomicUsize::new(0), n: 100 });
        let report = run_load(&plan, exec).await;
        assert_eq!(report.total, 3);
    }

    #[tokio::test]
    async fn duration_mode_runs_for_the_window() {
        let plan = LoadPlan::duration_ms(4, 80).expect("plan");
        // n=usize::MAX keeps the fast spin loop always succeeding, so the failure threshold never trips.
        let exec = Arc::new(EveryNthFails { calls: AtomicUsize::new(0), n: usize::MAX });
        let report = run_load(&plan, exec).await;
        assert!(report.total > 0, "duration mode should run at least a few times: {report:?}");
        assert!(report.elapsed_ms >= 70, "elapsed should be ≈ the duration: {}", report.elapsed_ms);
        assert_eq!(report.failed, 0);
    }
}
