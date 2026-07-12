//! Redis Streams work queue.
//!
//! Consumer groups must exist before any XADD (a `$` start position would miss
//! already-enqueued messages), so connect pre-creates a group per capability.

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
// Index of runtime names that ever had a targeted stream; reclaim_dead walks it.
const RT_INDEX: &str = "fleet:rt:index";
const SEP: char = '\u{1f}';

fn known_caps() -> [ExecutorKind; 4] {
    ExecutorKind::ALL
}

fn stream_key(k: ExecutorKind) -> String {
    format!("fleet:s:{}", k.as_str())
}

// Targeted stream: one per runtime name; only the runtime with that name reads it.
fn rt_stream_key(name: &str) -> String {
    format!("fleet:rt:{name}")
}

// XREADGROUP fails the whole call with NOGROUP if any key lacks the group, so ensure
// it exists (idempotent) before reading or writing a targeted stream.
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
    // claim opens a dedicated connection from this client for the blocking XREADGROUP:
    // blocking commands must never run on the shared multiplexed connection, or they
    // stall every XADD / concurrent claim on it (observed: each claim waited the full 20s).
    client: redis::Client,
    default_consumer: String,
}

impl RedisStreamQueue {
    pub async fn connect(
        url: &str,
        default_consumer: &str,
    ) -> Result<Arc<Self>, redis::RedisError> {
        let client = redis::Client::open(url)?;
        let mut conn = client.get_multiplexed_async_connection().await?;
        for cap in known_caps() {
            let key = stream_key(cap);
            // Existing group yields BUSYGROUP; ignore for idempotency.
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
        // Targeted specs go to the runtime's own stream, the rest to the capability stream.
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
        // Own targeted stream goes first so it is read before the shared capability streams.
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

        // Dedicated connection for the blocking XREADGROUP (never the shared self.conn,
        // which would stall enqueue / concurrent claims).
        let mut conn = self.client.get_multiplexed_async_connection().await.ok()?;
        let reply: StreamReadReply = conn.xread_options(&keys, &ids, &opts).await.ok()?;
        let (key, entry) =
            reply.keys.into_iter().find_map(|k| k.ids.into_iter().next().map(|e| (k.key, e)))?;
        let json: String = entry.get("spec")?;
        let wire: WireSpec = serde_json::from_str(&json).ok()?;
        let spec: WorkSpec = wire.into();
        // Ack map: attempt_id -> "<streamkey>\x1f<entryid>", used to locate the entry
        // for XACK on terminal state.
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

    // Reclaim only PEL entries whose holder is dead (not in `live`) and idle past
    // `grace`: re-XADD then XACK the old entry. A live holder is skipped even for
    // long-running work, to avoid re-dispatching a task that is still executing.
    // Targeted streams are reclaimed too (re-added to the same targeted stream):
    // a runtime that drops and reconnects gets a new id, so PEL entries under the
    // old id would otherwise be claimable by no one, forever.
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
                // Original payload gone (entry trimmed by MAXLEN): XACK to clear the
                // PEL without re-enqueueing.
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

    // XINFO GROUPS: lag = ready, pending = in_flight; a capability reports 0 on error.
    async fn stats(&self) -> Vec<QueueStat> {
        let mut conn = self.conn.clone();
        let mut out = Vec::with_capacity(known_caps().len());
        for cap in known_caps() {
            let key = stream_key(cap);
            let groups: StreamInfoGroupsReply = match conn.xinfo_groups(&key).await {
                Ok(g) => g,
                Err(_) => {
                    out.push(QueueStat {
                        executor: cap,
                        ready: 0,
                        in_flight: 0,
                        oldest_in_flight_ms: 0,
                    });
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

// WorkSpec has no serde derive, so WireSpec bridges the wire encoding.
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
    // default: tolerate entries enqueued before this field existed.
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
