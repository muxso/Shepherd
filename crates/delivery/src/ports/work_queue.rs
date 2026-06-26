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
    // 后端自身时钟所计(Redis 下为 XPENDING idle),不可跨进程比较。
    pub oldest_in_flight_ms: u64,
}

#[async_trait]
pub trait WorkQueue: Send + Sync {
    async fn enqueue(&self, spec: &WorkSpec);

    // 阻塞到 wait 超时的长轮询;consumer = runtime id,用于 PEL 归属与死 runtime 回收。
    async fn claim(&self, caps: &[ExecutorKind], wait: Duration, consumer: &str) -> Option<Claimed>;

    // 终态时调用,把消息移出 PEL,避免被 reclaim_dead 重投。内存实现为 no-op。
    async fn ack(&self, attempt_id: &str);

    // 重投持有者已不在 live 且空闲超 grace 的待处理任务;live 由注册表心跳判定。
    async fn reclaim_dead(&self, _live: &[String], _grace: Duration) -> usize {
        0
    }

    async fn stats(&self) -> Vec<QueueStat> {
        Vec::new()
    }
}
