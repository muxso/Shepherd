//! 消费组必须在任何 XADD 之前存在(否则 `$` 起点会漏掉已入队消息),故 connect 时预建全部能力组。

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

use redis::aio::MultiplexedConnection;
use redis::streams::{
    StreamInfoGroupsReply, StreamMaxlen, StreamPendingCountReply, StreamRangeReply,
    StreamReadOptions, StreamReadReply,
};
use redis::AsyncCommands;

const STREAM_MAXLEN: usize = 10_000;

use crate::domain::ExecutorKind;
use crate::ports::{Claimed, QueueStat, WorkQueue, WorkSpec};

const GROUP: &str = "fleet";
const ACKMAP: &str = "fleet:ackmap";
// 出现过定向流的 runtime name 索引,reclaim_dead 据此遍历定向流。
const RT_INDEX: &str = "fleet:rt:index";
const SEP: char = '\u{1f}';

fn known_caps() -> [ExecutorKind; 4] {
    ExecutorKind::ALL
}

fn stream_key(k: ExecutorKind) -> String {
    format!("fleet:s:{}", k.as_str())
}

// 定向流:按 runtime name 一流一名,只有该 name 的 runtime 会读它。
fn rt_stream_key(name: &str) -> String {
    format!("fleet:rt:{name}")
}

// XREADGROUP 碰到无组的 key 会整体 NOGROUP 报错,故读/写定向流前都要确保组存在(幂等)。
async fn ensure_group(conn: &mut MultiplexedConnection, key: &str) {
    let res: redis::RedisResult<()> = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(key)
        .arg(GROUP)
        .arg("$")
        .arg("MKSTREAM")
        .query_async(conn)
        .await;
    if let Err(e) = res {
        debug_assert!(e.to_string().contains("BUSYGROUP"), "unexpected xgroup error: {e}");
    }
}

pub struct RedisStreamQueue {
    conn: MultiplexedConnection,
    // claim 用它另开专用连接跑阻塞 XREADGROUP:阻塞命令绝不能跑在共享多路复用连接上,
    // 否则会把同连接上的 XADD/其它 claim 全卡到超时(实测每次认领要等满 20s)。
    client: redis::Client,
    default_consumer: String,
}

impl RedisStreamQueue {
    pub async fn connect(url: &str, default_consumer: &str) -> Result<Arc<Self>, redis::RedisError> {
        let client = redis::Client::open(url)?;
        let mut conn = client.get_multiplexed_async_connection().await?;
        for cap in known_caps() {
            let key = stream_key(cap);
            // 组已存在 → BUSYGROUP,幂等忽略。
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
        // 定向任务进该 runtime 专属流,其余进能力流。
        let key = match &spec.target_runtime {
            Some(name) => rt_stream_key(name),
            None => stream_key(spec.executor),
        };
        let json = match serde_json::to_string(&WireSpec::from(spec)) {
            Ok(j) => j,
            Err(_) => return,
        };
        let mut conn = self.conn.clone();
        if let Some(name) = &spec.target_runtime {
            ensure_group(&mut conn, &key).await;
            let _: redis::RedisResult<()> = conn.sadd(RT_INDEX, name).await;
        }
        let _: redis::RedisResult<String> = conn
            .xadd_maxlen(&key, StreamMaxlen::Approx(STREAM_MAXLEN), "*", &[("spec", json.as_str())])
            .await;
    }

    async fn claim(
        &self,
        caps: &[ExecutorKind],
        wait: Duration,
        consumer: &str,
        consumer_name: &str,
    ) -> Option<Claimed> {
        if caps.is_empty() {
            return None;
        }
        let consumer = if consumer.is_empty() { self.default_consumer.as_str() } else { consumer };
        let mut keys: Vec<String> = Vec::with_capacity(caps.len() + 1);
        // 自己名下的定向流排最前,优先于公共能力流被读到。
        if !consumer_name.is_empty() {
            let rt_key = rt_stream_key(consumer_name);
            let mut conn = self.conn.clone();
            ensure_group(&mut conn, &rt_key).await;
            keys.push(rt_key);
        }
        keys.extend(caps.iter().copied().map(stream_key));
        let ids: Vec<&str> = keys.iter().map(|_| ">").collect();
        let opts = StreamReadOptions::default()
            .group(GROUP, consumer)
            .block(wait.as_millis() as usize)
            .count(1);

        // 专用连接跑阻塞 XREADGROUP(勿用共享 self.conn,否则卡死 enqueue/并发 claim)。
        let mut conn = self.client.get_multiplexed_async_connection().await.ok()?;
        let reply: StreamReadReply = conn.xread_options(&keys, &ids, &opts).await.ok()?;
        let (key, entry) =
            reply.keys.into_iter().find_map(|k| k.ids.into_iter().next().map(|e| (k.key, e)))?;
        let json: String = entry.get("spec")?;
        let wire: WireSpec = serde_json::from_str(&json).ok()?;
        let spec: WorkSpec = wire.into();
        // ack 映射 attempt_id → "<streamkey>\x1f<entryid>",终态 XACK 时据此定位条目。
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

    // 只回收持有者已死(∉ live)且空闲超 grace 的 PEL 条目:重新 XADD + XACK 旧条目。
    // 持有者仍在线则跳过(长任务也算在线),避免重投正在跑的任务。
    // 定向流同样回收(重投回原定向流):runtime 掉线重连后拿新 id,旧 id 的 PEL 条目
    // 不回收就永远没人能再领到。
    async fn reclaim_dead(&self, live: &[String], grace: Duration) -> usize {
        let grace_ms = grace.as_millis() as usize;
        let mut conn = self.conn.clone();
        let mut requeued = 0usize;
        let mut keys: Vec<String> = known_caps().into_iter().map(stream_key).collect();
        let rt_names: Vec<String> = conn.smembers(RT_INDEX).await.unwrap_or_default();
        keys.extend(rt_names.iter().map(|n| rt_stream_key(n)));
        for key in keys {
            let pending: StreamPendingCountReply =
                match conn.xpending_count(&key, GROUP, "-", "+", 100usize).await {
                    Ok(p) => p,
                    Err(_) => continue,
                };
            for p in pending.ids {
                if live.iter().any(|l| l == &p.consumer) || p.last_delivered_ms < grace_ms {
                    continue;
                }
                // 取不到原 payload(条目已被 MAXLEN 裁剪)→ 直接 XACK 清出 PEL,不重投。
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

    // XINFO GROUPS 的 lag = ready,pending = in_flight;取不到则该能力计 0。
    async fn stats(&self) -> Vec<QueueStat> {
        let mut conn = self.conn.clone();
        let mut out = Vec::with_capacity(known_caps().len());
        for cap in known_caps() {
            let key = stream_key(cap);
            let groups: StreamInfoGroupsReply = match conn.xinfo_groups(&key).await {
                Ok(g) => g,
                Err(_) => {
                    out.push(QueueStat { executor: cap, ready: 0, in_flight: 0, oldest_in_flight_ms: 0 });
                    continue;
                }
            };
            let g = groups.groups.iter().find(|g| g.name == GROUP);
            let ready = g.and_then(|g| g.lag).unwrap_or(0) as u64;
            let in_flight = g.map(|g| g.pending).unwrap_or(0) as u64;
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

// WorkSpec 无 serde derive,故用 WireSpec 桥接线缆编解码。
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
    // default:兼容升级前已入队的旧条目(无此字段)。
    #[serde(default)]
    target_runtime: Option<String>,
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
            target_runtime: s.target_runtime.clone(),
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
            target_runtime: w.target_runtime,
        }
    }
}
