//! 分布式工作队列:Redis Streams + 消费组(feature = "exec-queue-redis")。
//!
//! 为什么 Streams:多 server 副本 + 多 runtime 下需要「外部化队列 + 恰好一台认领(一次性)
//! + 终态 ack + 死 runtime 超时回收」。消费组天然给到前三点:
//! - 每能力一条 stream(`fleet:s:CLAUDE_CODE`),固定消费组 `fleet`;
//! - `XREADGROUP … BLOCK` = 原生阻塞长轮询,一条消息只投一个 consumer,留在 PEL 直到 `XACK`;
//! - 终态(complete/fail/stop)经 `DeliveryService::ack` → `XACK`,移出 PEL。
//!
//! **未做(阶段 2)**:`XAUTOCLAIM` reaper —— 回收死 runtime 未 ack 的 pending 并重投;
//! 需配合心跳判活。现阶段崩溃任务会滞留 PEL(不会被正常重投,但也不自动回收)。
//!
//! 注意:消费组**必须在任何 XADD 之前存在**(否则 `$` 起点会漏掉已入队消息),
//! 故 `connect` 时为所有已知能力预建组。

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

use redis::aio::MultiplexedConnection;
use redis::streams::{
    StreamInfoGroupsReply, StreamMaxlen, StreamPendingCountReply, StreamRangeReply,
    StreamReadOptions, StreamReadReply,
};
use redis::AsyncCommands;

/// 流上限(近似裁剪),防止 XACK 后条目无限累积。
const STREAM_MAXLEN: usize = 10_000;

use crate::domain::ExecutorKind;
use crate::ports::{Claimed, QueueStat, WorkQueue, WorkSpec};

const GROUP: &str = "fleet";
const ACKMAP: &str = "fleet:ackmap";
const SEP: char = '\u{1f}';

/// 所有已知执行者能力(预建消费组用)。
fn known_caps() -> [ExecutorKind; 3] {
    [ExecutorKind::ClaudeCode, ExecutorKind::Codex, ExecutorKind::OpenCode]
}

fn stream_key(k: ExecutorKind) -> String {
    format!("fleet:s:{}", k.as_str())
}

pub struct RedisStreamQueue {
    /// 共享多路复用连接:enqueue/ack/reclaim 等**非阻塞**命令用。
    conn: MultiplexedConnection,
    /// 保留 client 以便 `claim` 开**专用连接**跑阻塞 XREADGROUP —— 阻塞命令绝不能跑在共享
    /// 多路复用连接上(会把同连接上的 XADD/其它 claim 全部卡到超时,实测每次认领要等满 20s)。
    client: redis::Client,
    /// 缺省消费者名(`claim` 未带 runtime id 时回退)。
    default_consumer: String,
}

impl RedisStreamQueue {
    /// 连接并为所有已知能力预建消费组(幂等,忽略 BUSYGROUP)。
    pub async fn connect(url: &str, default_consumer: &str) -> Result<Arc<Self>, redis::RedisError> {
        let client = redis::Client::open(url)?;
        let mut conn = client.get_multiplexed_async_connection().await?;
        for cap in known_caps() {
            let key = stream_key(cap);
            // XGROUP CREATE <key> <group> $ MKSTREAM —— 已存在则 BUSYGROUP,忽略。
            let res: redis::RedisResult<()> = redis::cmd("XGROUP")
                .arg("CREATE")
                .arg(&key)
                .arg(GROUP)
                .arg("$")
                .arg("MKSTREAM")
                .query_async(&mut conn)
                .await;
            if let Err(e) = res {
                if !e.to_string().contains("BUSYGROUP") {
                    return Err(e);
                }
            }
        }
        Ok(Arc::new(Self { conn, client, default_consumer: default_consumer.to_string() }))
    }
}

#[async_trait]
impl WorkQueue for RedisStreamQueue {
    async fn enqueue(&self, spec: &WorkSpec) {
        let key = stream_key(spec.executor);
        let json = match serde_json::to_string(&WireSpec::from(spec)) {
            Ok(j) => j,
            Err(_) => return,
        };
        let mut conn = self.conn.clone();
        // XADD <key> MAXLEN ~ 10000 * spec <json>
        let _: redis::RedisResult<String> = conn
            .xadd_maxlen(&key, StreamMaxlen::Approx(STREAM_MAXLEN), "*", &[("spec", json.as_str())])
            .await;
    }

    async fn claim(&self, caps: &[ExecutorKind], wait: Duration, consumer: &str) -> Option<Claimed> {
        if caps.is_empty() {
            return None;
        }
        let consumer = if consumer.is_empty() { self.default_consumer.as_str() } else { consumer };
        let keys: Vec<String> = caps.iter().copied().map(stream_key).collect();
        let ids: Vec<&str> = keys.iter().map(|_| ">").collect();
        let opts = StreamReadOptions::default()
            .group(GROUP, consumer)
            .block(wait.as_millis() as usize)
            .count(1);

        // 专用连接跑阻塞 XREADGROUP(勿用共享 self.conn,否则卡死 enqueue/并发 claim)。
        let mut conn = self.client.get_multiplexed_async_connection().await.ok()?;
        let reply: StreamReadReply = conn.xread_options(&keys, &ids, &opts).await.ok()?;
        // 取首个非空流的首条消息(COUNT 1;多流时任取其一)。
        let (key, entry) =
            reply.keys.into_iter().find_map(|k| k.ids.into_iter().next().map(|e| (k.key, e)))?;
        let json: String = entry.get("spec")?;
        let wire: WireSpec = serde_json::from_str(&json).ok()?;
        let spec: WorkSpec = wire.into();
        // 记 ack 映射:attempt_id → "<streamkey>\x1f<entryid>",供终态 XACK。
        let val = format!("{}{}{}", key, SEP, entry.id);
        let _: redis::RedisResult<()> = conn.hset(ACKMAP, &spec.attempt_id, val).await;
        Some(Claimed { spec })
    }

    async fn ack(&self, attempt_id: &str) {
        let mut conn = self.conn.clone();
        let val: Option<String> = conn.hget(ACKMAP, attempt_id).await.ok().flatten();
        let Some(val) = val else { return };
        let Some((key, id)) = val.split_once(SEP) else { return };
        let _: redis::RedisResult<i64> = conn.xack(key, GROUP, &[id]).await;
        let _: redis::RedisResult<()> = conn.hdel(ACKMAP, attempt_id).await;
    }

    /// 扫各能力流的 PEL,凡「持有者 ∉ `live`(死 runtime)且空闲超 `grace`」的待处理消息
    /// → 重新 XADD 入队 + XACK 旧条目。持有者仍在线 → 不回收(哪怕长任务空闲很久)。
    /// 不依赖 XAUTOCLAIM(用 XPENDING + XRANGE,typed helper 更稳)。
    async fn reclaim_dead(&self, live: &[String], grace: Duration) -> usize {
        let grace_ms = grace.as_millis() as usize;
        let mut conn = self.conn.clone();
        let mut requeued = 0usize;
        for cap in known_caps() {
            let key = stream_key(cap);
            let pending: StreamPendingCountReply =
                match conn.xpending_count(&key, GROUP, "-", "+", 100usize).await {
                    Ok(p) => p,
                    Err(_) => continue,
                };
            for p in pending.ids {
                // 持有者仍在线 → 跳过(正常处理中,含长任务);仅死 runtime 且过容忍期才回收。
                if live.iter().any(|l| l == &p.consumer) || p.last_delivered_ms < grace_ms {
                    continue;
                }
                // 取回原 payload;取不到(条目已被裁剪)→ 直接 ack 清出 PEL。
                let range: StreamRangeReply = match conn.xrange(&key, &p.id, &p.id).await {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                match range.ids.into_iter().next().and_then(|e| e.get::<String>("spec")) {
                    Some(json) => {
                        let _: redis::RedisResult<String> = conn
                            .xadd_maxlen(
                                &key,
                                StreamMaxlen::Approx(STREAM_MAXLEN),
                                "*",
                                &[("spec", json.as_str())],
                            )
                            .await;
                        let _: redis::RedisResult<i64> = conn.xack(&key, GROUP, &[&p.id]).await;
                        requeued += 1;
                    }
                    None => {
                        let _: redis::RedisResult<i64> = conn.xack(&key, GROUP, &[&p.id]).await;
                    }
                }
            }
        }
        requeued
    }

    /// 各能力流计数:`XINFO GROUPS` 取消费组 `fleet` 的 `lag`(待投递=ready)与 `pending`(在飞);
    /// 在飞 > 0 时再 `XPENDING` 取最久一条的空闲时长(stuck 信号,Redis 自身时钟所计)。
    /// 取不到(流不存在/旧版无 lag)→ 该项计 0,不阻断其余能力。
    async fn stats(&self) -> Vec<QueueStat> {
        let mut conn = self.conn.clone();
        let mut out = Vec::with_capacity(known_caps().len());
        for cap in known_caps() {
            let key = stream_key(cap);
            let groups: StreamInfoGroupsReply = match conn.xinfo_groups(&key).await {
                Ok(g) => g,
                // 流尚未建(无任何 XADD/组)→ 全 0。
                Err(_) => {
                    out.push(QueueStat { executor: cap, ready: 0, in_flight: 0, oldest_in_flight_ms: 0 });
                    continue;
                }
            };
            let g = groups.groups.iter().find(|g| g.name == GROUP);
            let ready = g.and_then(|g| g.lag).unwrap_or(0) as u64;
            let in_flight = g.map(|g| g.pending).unwrap_or(0) as u64;
            // 仅在有在飞任务时多打一次 XPENDING,取最久空闲(oldest pending 在区间起点)。
            let oldest_in_flight_ms = if in_flight > 0 {
                let pending: redis::RedisResult<StreamPendingCountReply> =
                    conn.xpending_count(&key, GROUP, "-", "+", 1usize).await;
                match pending {
                    Ok(p) => p.ids.first().map(|x| x.last_delivered_ms as u64).unwrap_or(0),
                    Err(_) => 0,
                }
            } else {
                0
            };
            out.push(QueueStat { executor: cap, ready, in_flight, oldest_in_flight_ms });
        }
        out
    }
}

// —— 线缆编解码:WorkSpec 无 serde derive,这里桥一层 ——

#[derive(serde::Serialize, serde::Deserialize)]
struct WireSpec {
    attempt_id: String,
    decomposition_id: String,
    task_id: String,
    title: String,
    description: String,
    acceptance_criteria: Vec<String>,
    executor: String,
    context: Option<String>,
    instructions: Option<String>,
}

impl From<&WorkSpec> for WireSpec {
    fn from(s: &WorkSpec) -> Self {
        Self {
            attempt_id: s.attempt_id.clone(),
            decomposition_id: s.decomposition_id.clone(),
            task_id: s.task_id.clone(),
            title: s.title.clone(),
            description: s.description.clone(),
            acceptance_criteria: s.acceptance_criteria.clone(),
            executor: s.executor.as_str().to_string(),
            context: s.context.clone(),
            instructions: s.instructions.clone(),
        }
    }
}

impl From<WireSpec> for WorkSpec {
    fn from(w: WireSpec) -> Self {
        WorkSpec {
            attempt_id: w.attempt_id,
            decomposition_id: w.decomposition_id,
            task_id: w.task_id,
            title: w.title,
            description: w.description,
            acceptance_criteria: w.acceptance_criteria,
            executor: ExecutorKind::parse(&w.executor).unwrap_or(ExecutorKind::ClaudeCode),
            context: w.context,
            instructions: w.instructions,
        }
    }
}
