use async_trait::async_trait;
use std::time::Duration;

use crate::domain::ExecutorKind;
use crate::ports::WorkSpec;

#[derive(Debug, Clone)]
pub struct Claimed {
    pub spec: WorkSpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueStat {
    pub executor: ExecutorKind,
    pub ready: u64,
    pub in_flight: u64,
    // Measured by the backend's own clock (XPENDING idle on Redis); not comparable
    // across processes.
    pub oldest_in_flight_ms: u64,
}

#[async_trait]
pub trait WorkQueue: Send + Sync {
    async fn enqueue(&self, spec: &WorkSpec);

    // Long poll blocking up to `wait`. consumer = runtime id (PEL ownership and
    // dead-runtime reclaim); consumer_name = registered runtime name (targeted-spec
    // matching: only the runtime with that name may claim).
    async fn claim(
        &self,
        caps: &[ExecutorKind],
        wait: Duration,
        consumer: &str,
        consumer_name: &str,
    ) -> Option<Claimed>;

    // Called on terminal state to move the message out of the PEL so reclaim_dead
    // cannot re-dispatch it. No-op for the in-memory implementation.
    async fn ack(&self, attempt_id: &str);

    // Re-dispatch pending work whose holder is not in `live` and has been idle past
    // `grace`; `live` comes from registry heartbeats.
    async fn reclaim_dead(&self, _live: &[String], _grace: Duration) -> usize {
        0
    }

    async fn stats(&self) -> Vec<QueueStat> {
        Vec::new()
    }
}
