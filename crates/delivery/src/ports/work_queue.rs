//! 工作队列出站端口:把 WorkSpec 交给某种队列,远程 runtime 出站认领。
//!
//! 单机:进程内队列(`InMemoryWorkQueue`)。
//! 分布式(多 server 副本 + 多 runtime):需外部化、可靠的队列(`RedisStreamQueue`),
//! 满足「恰好一台 runtime 认领(一次性)+ 终态 ack + 死 runtime 超时回收」。

use async_trait::async_trait;
use std::time::Duration;

use crate::domain::ExecutorKind;
use crate::ports::WorkSpec;

/// 一次认领的结果:工作规格 + 不透明 ack 句柄(Redis 下为 stream entry id;内存下为空)。
#[derive(Debug, Clone)]
pub struct Claimed {
    pub spec: WorkSpec,
}

#[async_trait]
pub trait WorkQueue: Send + Sync {
    /// 入队一个待执行任务(dispatch 调用)。
    async fn enqueue(&self, spec: &WorkSpec);

    /// 认领一个匹配 `caps` 的任务;无则阻塞到 `wait` 超时(长轮询)。
    /// `consumer` = 认领方身份(= runtime id),用于 PEL 归属与死 runtime 回收。
    async fn claim(&self, caps: &[ExecutorKind], wait: Duration, consumer: &str) -> Option<Claimed>;

    /// 任务落终态时确认(移出待处理,避免被回收重投)。内存实现为 no-op。
    async fn ack(&self, attempt_id: &str);

    /// 回收「持有者已不在 `live`(死 runtime)且空闲超 `grace`」的待处理任务,重新入队。
    /// 返回重投条数。`live` = 在线 runtime id 集合(由注册表心跳判定)。内存队列默认 no-op。
    async fn reclaim_dead(&self, _live: &[String], _grace: Duration) -> usize {
        0
    }
}
