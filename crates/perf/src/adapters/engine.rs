//! 原生压测引擎(tokio):按负载计划并发跑,采样后交 domain 聚合成报告。
//!
//! 把 `iterations` 均摊到 `concurrency` 个并发 worker(无共享计数器,确定性、无竞争);
//! 每次调用 `RequestExecutor` 用墙钟测延迟。这就是 NativePerfDispatcher 的执行核心。

use std::sync::Arc;
use std::time::Instant;

use crate::domain::{aggregate, LoadPlan, LoadReport, Sample};
use crate::ports::RequestExecutor;

/// 跑一轮压测:返回聚合报告(吞吐/错误率/延迟分位)。
pub async fn run_load(plan: &LoadPlan, exec: Arc<dyn RequestExecutor>) -> LoadReport {
    let base = plan.iterations / plan.concurrency;
    let extra = plan.iterations % plan.concurrency;

    let start = Instant::now();
    let mut handles = Vec::with_capacity(plan.concurrency);
    for i in 0..plan.concurrency {
        // 前 `extra` 个 worker 多担一次,合计正好 iterations 次。
        let count = base + usize::from(i < extra);
        if count == 0 {
            continue;
        }
        let exec = exec.clone();
        handles.push(tokio::spawn(async move {
            let mut local = Vec::with_capacity(count);
            for _ in 0..count {
                let t = Instant::now();
                let success = exec.execute().await;
                local.push(Sample::new(t.elapsed().as_millis() as u64, success));
            }
            local
        }));
    }

    let mut samples = Vec::with_capacity(plan.iterations);
    for h in handles {
        if let Ok(mut local) = h.await {
            samples.append(&mut local);
        }
    }
    let elapsed_ms = start.elapsed().as_millis() as u64;
    aggregate(&samples, elapsed_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 每第 N 次调用失败一次,用于验证计数/错误率聚合。
    struct EveryNthFails {
        calls: AtomicUsize,
        n: usize,
    }
    #[async_trait]
    impl RequestExecutor for EveryNthFails {
        async fn execute(&self) -> bool {
            let i = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            i % self.n != 0 // 第 n、2n、… 次失败
        }
    }

    #[tokio::test]
    async fn runs_exact_iterations_and_aggregates() {
        let plan = LoadPlan::new(4, 20).expect("plan");
        let exec = Arc::new(EveryNthFails { calls: AtomicUsize::new(0), n: 5 });
        let report = run_load(&plan, exec).await;
        assert_eq!(report.total, 20); // 正好跑满
        assert_eq!(report.failed, 4); // 每 5 次失败 1 → 20/5
        assert_eq!(report.success, 16);
        assert!((report.error_rate - 0.2).abs() < 1e-9);
        assert!(report.throughput_rps >= 0.0);
    }

    #[tokio::test]
    async fn uneven_split_still_runs_all() {
        // 7 次 / 4 并发 = 2,2,1,1,0…→ 前 3 个 worker 多担
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
}
