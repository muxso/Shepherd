//! Resource-pool remote dispatch over WebSocket.
//!
//! Runners have no public network, so every connection is outbound from the
//! runner: it dials GET /api/pool-runner/ws, sends a hello and stays connected.
//! The server pushes compiled scenario plans down the same socket and ingests
//! the step-event stream into the regular report sinks, so remote reports are
//! indistinguishable from local ones. Browsers subscribe to the relayed events
//! on GET /api/run-events/ws to animate live per-step status.
//!
//! State is in-memory only (registry + short-lived event history); a server
//! restart simply drops runners, which reconnect with backoff.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{close_code, CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot};

use api_scenario::application::RecordScenarioExecutionUseCase;
use api_test::adapters::local::CaseResultSink;
use api_test::adapters::plan::StepObserver;
use api_test::adapters::PgBatchReport;
use pool_runner::protocol::{RunnerMsg, ServerMsg, WireEnv, WireNode};
use webauth::SessionStore;

const HEARTBEAT_EVERY: Duration = Duration::from_secs(15);
const LIVENESS_TIMEOUT: Duration = Duration::from_secs(45);
/// Event history kept after run completion so late subscribers still replay.
const DONE_RETENTION: Duration = Duration::from_secs(300);
const STALE_RETENTION: Duration = Duration::from_secs(1800);
const HISTORY_CAP: usize = 2000;
/// Max runs waiting per pool when every runner is at its concurrency cap;
/// overflow falls back to local execution.
const QUEUE_CAP: usize = 64;

// ---------------------------------------------------------------------------
// Browser-facing run event hub
// ---------------------------------------------------------------------------

struct RunChannel {
    tx: broadcast::Sender<String>,
    history: Vec<String>,
    created: Instant,
    done_at: Option<Instant>,
}

/// Per-run fan-out of live step events, with bounded replay history so a
/// browser that connects right after the run started still sees every event.
#[derive(Default)]
pub struct RunEventHub {
    runs: Mutex<HashMap<String, RunChannel>>,
}

impl RunEventHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(&self, run_id: &str, event: serde_json::Value) {
        let done = event.get("type").and_then(|t| t.as_str()) == Some("runComplete");
        let line = event.to_string();
        let mut runs = self.runs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let ch = runs.entry(run_id.to_string()).or_insert_with(|| RunChannel {
            tx: broadcast::channel(256).0,
            history: Vec::new(),
            created: Instant::now(),
            done_at: None,
        });
        if ch.history.len() < HISTORY_CAP {
            ch.history.push(line.clone());
        }
        if done {
            ch.done_at = Some(Instant::now());
        }
        let _ = ch.tx.send(line);
    }

    /// History snapshot + live receiver. Creates the channel if the run has not
    /// emitted yet (subscriber raced the first event).
    pub fn subscribe(&self, run_id: &str) -> (Vec<String>, broadcast::Receiver<String>) {
        let mut runs = self.runs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let ch = runs.entry(run_id.to_string()).or_insert_with(|| RunChannel {
            tx: broadcast::channel(256).0,
            history: Vec::new(),
            created: Instant::now(),
            done_at: None,
        });
        (ch.history.clone(), ch.tx.subscribe())
    }

    fn gc(&self) {
        let mut runs = self.runs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        runs.retain(|_, ch| match ch.done_at {
            Some(d) => d.elapsed() < DONE_RETENTION,
            None => ch.created.elapsed() < STALE_RETENTION,
        });
    }
}

/// Bridges the in-process executor's step lifecycle into the event hub, so
/// local (non-pool) runs animate in the browser exactly like remote ones.
pub struct HubObserver {
    hub: Arc<RunEventHub>,
}

impl HubObserver {
    pub fn new(hub: Arc<RunEventHub>) -> Self {
        Self { hub }
    }
}

impl StepObserver for HubObserver {
    fn step_started(&self, report_id: &str, step_id: &str) {
        self.hub.publish(
            report_id,
            serde_json::json!({"type": "stepStarted", "runId": report_id, "stepId": step_id}),
        );
    }

    fn step_finished(&self, report_id: &str, step_id: &str, outcome: &str, latency: Option<u64>) {
        self.hub.publish(
            report_id,
            serde_json::json!({
                "type": "stepFinished", "runId": report_id, "stepId": step_id,
                "status": outcome, "latencyMs": latency,
            }),
        );
    }
}

// ---------------------------------------------------------------------------
// Runner registry + remote run bookkeeping
// ---------------------------------------------------------------------------

/// What the ingest side needs to persist remote results through the same sinks
/// as local execution.
#[derive(Clone)]
pub struct RemoteDeps {
    pub sink: Arc<dyn CaseResultSink>,
    pub reports: PgBatchReport,
    pub recorder: RecordScenarioExecutionUseCase,
}

struct RunnerEntry {
    pool_id: String,
    name: String,
    /// Concurrent-run cap from the runner's hello; never assign beyond it.
    max_concurrent: usize,
    /// Capability tags from the runner's hello (e.g. "http", "browser").
    capabilities: Vec<String>,
    tx: mpsc::UnboundedSender<ServerMsg>,
}

/// Capability match: the runner must advertise every required tag; a run with
/// no requirements matches any runner.
fn runner_eligible(entry: &RunnerEntry, required: &[String]) -> bool {
    required.iter().all(|t| entry.capabilities.iter().any(|c| c == t))
}

/// Context needed to finalize a remote run (report status + execution record).
pub struct RunCtx {
    pub scenario_id: String,
    pub project_id: String,
    pub case_count: i32,
    /// Report the step results persist into. Equals the dispatch run id for
    /// single runs; union batch members share one report under distinct run ids.
    pub report_id: String,
    /// false = the caller owns the report status (union batch: one report,
    /// many member runs); the hub then only records the execution.
    pub finalize_report: bool,
}

/// How a dispatched-or-queued run ended, from the waiter's point of view.
pub enum RemoteOutcome {
    /// Remote execution finished with this report status.
    Finished(String),
    /// Never started remotely (pool lost its runners while queued); the caller
    /// should execute locally.
    Fallback,
}

struct Pending {
    runner_id: String,
    ctx: RunCtx,
    done: Option<oneshot::Sender<RemoteOutcome>>,
}

/// Run accepted while every capable pool runner was at its cap; dispatched
/// FIFO as capacity frees up.
struct QueuedRun {
    run_id: String,
    /// Capability tags the run requires; only matching runners are assigned.
    required: Vec<String>,
    nodes: Vec<WireNode>,
    env: WireEnv,
    stop_on_failure: bool,
    ctx: RunCtx,
    done: oneshot::Sender<RemoteOutcome>,
}

struct HubState {
    runners: HashMap<String, RunnerEntry>,
    pending: HashMap<String, Pending>,
    queues: HashMap<String, VecDeque<QueuedRun>>,
    seq: u64,
}

/// One connected runner in the detailed status endpoint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerStatus {
    pub name: String,
    pub max_concurrent: usize,
    pub in_flight: usize,
}

/// Least-busy capability-eligible runner of the pool with spare capacity.
fn pick_runner(st: &HubState, pool_id: &str, required: &[String]) -> Option<String> {
    let mut busy: HashMap<&str, usize> = HashMap::new();
    for p in st.pending.values() {
        *busy.entry(p.runner_id.as_str()).or_default() += 1;
    }
    let load = |id: &str| busy.get(id).copied().unwrap_or(0);
    st.runners
        .iter()
        .filter(|(id, r)| {
            r.pool_id == pool_id && runner_eligible(r, required) && load(id) < r.max_concurrent
        })
        .min_by_key(|(id, _)| load(id))
        .map(|(id, _)| id.clone())
}

/// Connected-runner registry plus in-flight remote runs.
pub struct PoolHub {
    state: Mutex<HubState>,
    deps: RemoteDeps,
    events: Arc<RunEventHub>,
}

impl PoolHub {
    pub fn new(deps: RemoteDeps, events: Arc<RunEventHub>) -> Arc<Self> {
        let hub = Arc::new(Self {
            state: Mutex::new(HubState {
                runners: HashMap::new(),
                pending: HashMap::new(),
                queues: HashMap::new(),
                seq: 0,
            }),
            deps,
            events,
        });
        // Single background GC for the event history.
        let ev = hub.events.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            loop {
                tick.tick().await;
                ev.gc();
            }
        });
        hub
    }

    pub fn events(&self) -> Arc<RunEventHub> {
        self.events.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HubState> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn register(
        &self,
        pool_id: &str,
        name: &str,
        max_concurrent: usize,
        capabilities: Vec<String>,
        tx: mpsc::UnboundedSender<ServerMsg>,
    ) -> String {
        let id = {
            let mut st = self.lock();
            st.seq += 1;
            let id = format!("pr-{}-{}", st.seq, &uuid::Uuid::new_v4().to_string()[..8]);
            st.runners.insert(
                id.clone(),
                RunnerEntry {
                    pool_id: pool_id.to_string(),
                    name: name.to_string(),
                    max_concurrent: max_concurrent.max(1),
                    capabilities,
                    tx,
                },
            );
            id
        };
        tracing::info!(runner = %id, pool = %pool_id, %name, max_concurrent, "pool runner connected");
        // New capacity may unblock queued runs of this pool.
        self.drain(pool_id);
        id
    }

    /// Removes the runner and fails all of its in-flight runs. Queued runs are
    /// re-dispatched to surviving runners, or handed back for local execution
    /// when the pool went empty.
    async fn unregister(&self, runner_id: &str) {
        let (orphans, flush, drain_pool) = {
            let mut st = self.lock();
            let pool_id = st.runners.remove(runner_id).map(|r| r.pool_id);
            if pool_id.is_some() {
                tracing::info!(runner = %runner_id, "pool runner disconnected");
            }
            let orphans: Vec<String> = st
                .pending
                .iter()
                .filter(|(_, p)| p.runner_id == runner_id)
                .map(|(rid, _)| rid.clone())
                .collect();
            match pool_id {
                Some(pid) if st.runners.values().any(|r| r.pool_id == pid) => {
                    (orphans, VecDeque::new(), Some(pid))
                }
                Some(pid) => (orphans, st.queues.remove(&pid).unwrap_or_default(), None),
                None => (orphans, VecDeque::new(), None),
            }
        };
        for run_id in orphans {
            self.finish(&run_id, "ERROR", Some("runner disconnected")).await;
        }
        for job in flush {
            tracing::info!(run = %job.run_id, "queued run falls back to local execution (pool has no runner)");
            let _ = job.done.send(RemoteOutcome::Fallback);
        }
        if let Some(pid) = drain_pool {
            self.drain(&pid);
        }
    }

    /// Assigns queued runs of the pool while spare capacity exists. A front
    /// job whose required capabilities no runner advertises anymore is handed
    /// back for local execution instead of blocking the queue.
    fn drain(&self, pool_id: &str) {
        loop {
            let mut st = self.lock();
            let Some(required) =
                st.queues.get(pool_id).and_then(VecDeque::front).map(|j| j.required.clone())
            else {
                st.queues.remove(pool_id);
                return;
            };
            if !st.runners.values().any(|r| r.pool_id == pool_id && runner_eligible(r, &required)) {
                let job = st.queues.get_mut(pool_id).and_then(VecDeque::pop_front);
                drop(st);
                if let Some(job) = job {
                    tracing::info!(run = %job.run_id, pool = %pool_id, "queued run falls back to local execution (no capable runner)");
                    let _ = job.done.send(RemoteOutcome::Fallback);
                }
                continue;
            }
            let Some(rid) = pick_runner(&st, pool_id, &required) else { return };
            let Some((name, tx)) = st.runners.get(&rid).map(|e| (e.name.clone(), e.tx.clone()))
            else {
                return;
            };
            let Some(job) = st.queues.get_mut(pool_id).and_then(VecDeque::pop_front) else {
                return;
            };
            let QueuedRun { run_id, required, nodes, env, stop_on_failure, ctx, done } = job;
            let msg = ServerMsg::Run { run_id: run_id.clone(), nodes, env, stop_on_failure };
            if let Err(mpsc::error::SendError(msg)) = tx.send(msg) {
                // Runner died between liveness checks: drop it, requeue the job.
                st.runners.remove(&rid);
                if let ServerMsg::Run { run_id, nodes, env, stop_on_failure } = msg {
                    st.queues.entry(pool_id.to_string()).or_default().push_front(QueuedRun {
                        run_id,
                        required,
                        nodes,
                        env,
                        stop_on_failure,
                        ctx,
                        done,
                    });
                }
                continue;
            }
            tracing::info!(run = %run_id, runner = %rid, %name, pool = %pool_id, "queued run dispatched to pool runner");
            st.pending.insert(run_id, Pending { runner_id: rid, ctx, done: Some(done) });
        }
    }

    /// True when the pool has at least one live runner.
    pub fn has_runner(&self, pool_id: &str) -> bool {
        self.lock().runners.values().any(|r| r.pool_id == pool_id)
    }

    pub fn online_counts(&self) -> HashMap<String, usize> {
        let st = self.lock();
        let mut m: HashMap<String, usize> = HashMap::new();
        for r in st.runners.values() {
            *m.entry(r.pool_id.clone()).or_default() += 1;
        }
        m
    }

    /// Per-pool runner details: name, advertised cap and current in-flight runs.
    pub fn online_detail(&self) -> HashMap<String, Vec<RunnerStatus>> {
        let st = self.lock();
        let mut busy: HashMap<&str, usize> = HashMap::new();
        for p in st.pending.values() {
            *busy.entry(p.runner_id.as_str()).or_default() += 1;
        }
        let mut m: HashMap<String, Vec<RunnerStatus>> = HashMap::new();
        for (id, r) in &st.runners {
            m.entry(r.pool_id.clone()).or_default().push(RunnerStatus {
                name: r.name.clone(),
                max_concurrent: r.max_concurrent,
                in_flight: busy.get(id.as_str()).copied().unwrap_or(0),
            });
        }
        for v in m.values_mut() {
            v.sort_by(|a, b| a.name.cmp(&b.name));
        }
        m
    }

    /// Sends the compiled plan to the least-loaded runner of the pool that
    /// advertises every `required` capability tag and has spare capacity
    /// (empty `required` matches any runner); when every eligible runner is at
    /// its cap, the run is queued FIFO server-side. None = no eligible runner
    /// or the queue is full (caller falls back to in-process execution). The
    /// second tuple element is the assigned runner's display name (None while
    /// queued).
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch(
        &self,
        pool_id: &str,
        run_id: &str,
        required: Vec<String>,
        nodes: Vec<WireNode>,
        env: WireEnv,
        stop_on_failure: bool,
        ctx: RunCtx,
    ) -> Option<(oneshot::Receiver<RemoteOutcome>, Option<String>)> {
        let (done_tx, done_rx) = oneshot::channel();
        let mut st = self.lock();
        if !st.runners.values().any(|r| r.pool_id == pool_id && runner_eligible(r, &required)) {
            if st.runners.values().any(|r| r.pool_id == pool_id) {
                tracing::info!(run = %run_id, pool = %pool_id, ?required, "no pool runner advertises the required capabilities; falling back to local execution");
            }
            return None;
        }
        match pick_runner(&st, pool_id, &required) {
            Some(runner_id) => {
                let entry = st.runners.get(&runner_id)?;
                let name = entry.name.clone();
                let msg =
                    ServerMsg::Run { run_id: run_id.to_string(), nodes, env, stop_on_failure };
                if entry.tx.send(msg).is_err() {
                    return None;
                }
                tracing::info!(run = %run_id, runner = %runner_id, %name, pool = %pool_id, "scenario dispatched to pool runner");
                st.pending
                    .insert(run_id.to_string(), Pending { runner_id, ctx, done: Some(done_tx) });
                Some((done_rx, Some(name)))
            }
            None => {
                // Eligible runners exist but all are saturated: queue within bounds.
                let queue = st.queues.entry(pool_id.to_string()).or_default();
                if queue.len() >= QUEUE_CAP {
                    tracing::warn!(run = %run_id, pool = %pool_id, "pool queue full; falling back to local execution");
                    return None;
                }
                queue.push_back(QueuedRun {
                    run_id: run_id.to_string(),
                    required,
                    nodes,
                    env,
                    stop_on_failure,
                    ctx,
                    done: done_tx,
                });
                tracing::info!(run = %run_id, pool = %pool_id, depth = queue.len(), "pool saturated; run queued");
                Some((done_rx, None))
            }
        }
    }

    /// Report + execution record + browser completion event for one run.
    async fn record_outcome(&self, run_id: &str, status: &str, ctx: &RunCtx, reason: Option<&str>) {
        if let Some(r) = reason {
            tracing::warn!(run = %run_id, %status, reason = r, "remote run finalized");
        }
        if ctx.finalize_report {
            let _ = self.deps.reports.set_status(&ctx.report_id, status).await;
        }
        let _ = self
            .deps
            .recorder
            .execute(
                &ctx.scenario_id,
                &ctx.project_id,
                status,
                ctx.case_count,
                Some(&ctx.report_id),
            )
            .await;
        if ctx.finalize_report {
            self.events.publish(
                &ctx.report_id,
                serde_json::json!({"type": "runComplete", "runId": ctx.report_id, "status": status}),
            );
        }
    }

    /// Finalizes an in-flight remote run once: report status, execution record,
    /// completion event, waiter wake-up, then hands freed capacity to the
    /// queue. Later duplicate calls are no-ops.
    async fn finish(&self, run_id: &str, status: &str, reason: Option<&str>) {
        let (p, freed_pool) = {
            let mut st = self.lock();
            let Some(p) = st.pending.remove(run_id) else { return };
            let pool = st.runners.get(&p.runner_id).map(|r| r.pool_id.clone());
            (p, pool)
        };
        self.record_outcome(run_id, status, &p.ctx, reason).await;
        if let Some(tx) = p.done {
            let _ = tx.send(RemoteOutcome::Finished(status.to_string()));
        }
        if let Some(pool_id) = freed_pool {
            self.drain(&pool_id);
        }
    }

    /// Abandons a remote run from the waiter side (dispatch timeout), whether
    /// still queued or already in flight.
    pub async fn abort_run(&self, run_id: &str) {
        let queued = {
            let mut st = self.lock();
            let mut found = None;
            for q in st.queues.values_mut() {
                if let Some(pos) = q.iter().position(|j| j.run_id == run_id) {
                    found = q.remove(pos);
                    break;
                }
            }
            found
        };
        if let Some(job) = queued {
            // Waiter gave up; the done sender is dropped with the job.
            self.record_outcome(run_id, "ERROR", &job.ctx, Some("dispatch timeout")).await;
            return;
        }
        self.finish(run_id, "ERROR", Some("dispatch timeout")).await;
    }

    /// Report a runner event belongs to: union batch members persist under a
    /// shared report id distinct from their dispatch run id.
    fn report_of(&self, run_id: &str) -> String {
        self.lock()
            .pending
            .get(run_id)
            .map(|p| p.ctx.report_id.clone())
            .unwrap_or_else(|| run_id.to_string())
    }

    /// Ingests one event from a runner: persist through the local sinks and
    /// relay to subscribed browsers.
    async fn ingest(&self, msg: RunnerMsg) {
        match msg {
            RunnerMsg::Hello { .. } => {} // handled at connect
            RunnerMsg::StepStarted { run_id, step_id } => {
                let report_id = self.report_of(&run_id);
                self.events.publish(
                    &report_id,
                    serde_json::json!({"type": "stepStarted", "runId": report_id, "stepId": step_id}),
                );
            }
            RunnerMsg::StepFinished { run_id, step_id, outcome, failures } => {
                let report_id = self.report_of(&run_id);
                let _ = self.deps.sink.record(&report_id, &step_id, &outcome, &failures).await;
                self.events.publish(
                    &report_id,
                    serde_json::json!({
                        "type": "stepFinished", "runId": report_id, "stepId": step_id,
                        "status": outcome, "failures": failures,
                    }),
                );
            }
            RunnerMsg::StepDetail(d) => {
                let report_id = self.report_of(&d.run_id);
                let _ = self
                    .deps
                    .sink
                    .record_detail(
                        &report_id,
                        &d.step_id,
                        d.status_code,
                        d.latency_ms,
                        d.resp_size,
                        &d.body,
                        &d.headers,
                        &d.assertions,
                        &d.extractions,
                        &d.req_method,
                        &d.req_url,
                        &d.req_headers,
                        d.req_body.as_deref(),
                        &d.timings,
                    )
                    .await;
                self.events.publish(
                    &report_id,
                    serde_json::json!({
                        "type": "stepDetail", "runId": report_id, "stepId": d.step_id,
                        "statusCode": d.status_code, "latencyMs": d.latency_ms, "timings": d.timings,
                    }),
                );
            }
            RunnerMsg::RunComplete { run_id, all_pass } => {
                let status = if all_pass { "SUCCESS" } else { "ERROR" };
                self.finish(&run_id, status, None).await;
            }
            RunnerMsg::RunError { run_id, message } => {
                self.finish(&run_id, "ERROR", Some(&message)).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP/WS endpoints
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct WsState {
    hub: Arc<PoolHub>,
    sessions: Arc<dyn SessionStore>,
    /// Pool lookups at hello time (poolName resolution).
    pool: migrate::PgPool,
}

#[derive(Deserialize)]
struct WsQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default, rename = "runId")]
    run_id: Option<String>,
}

pub fn router(hub: Arc<PoolHub>, sessions: Arc<dyn SessionStore>, pool: migrate::PgPool) -> Router {
    Router::new()
        .route("/api/pool-runner/ws", get(runner_ws))
        .route("/api/pool-runner/status", get(runner_status))
        .route("/api/pool-runner/status/detail", get(runner_status_detail))
        .route("/api/run-events/ws", get(run_events_ws))
        .with_state(WsState { hub, sessions, pool })
}

/// Browsers can't set Authorization on WebSocket upgrades, so accept the token
/// from either the header or `?token=`.
async fn authorize(st: &WsState, headers: &HeaderMap, q: &WsQuery) -> Option<webauth::Session> {
    let header_token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(str::to_string);
    let token = header_token.or_else(|| q.token.clone())?;
    st.sessions.get(&token).await.ok().flatten()
}

async fn runner_ws(
    ws: WebSocketUpgrade,
    State(st): State<WsState>,
    Query(q): Query<WsQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(session) = authorize(&st, &headers, &q).await else {
        return (StatusCode::UNAUTHORIZED, "invalid or missing token").into_response();
    };
    // Runner keys must be allowed to execute scenarios.
    if !session.permissions.allows("API_SCENARIO", "EXECUTE") {
        return (StatusCode::FORBIDDEN, "key lacks API_SCENARIO:EXECUTE").into_response();
    }
    ws.on_upgrade(move |socket| runner_session(socket, st.hub, st.pool))
}

/// Hello poolId/poolName → registered pool id. Err = human-readable rejection
/// sent as the close reason (the runner logs it verbatim).
async fn resolve_hello_pool(
    db: &migrate::PgPool,
    pool_id: Option<String>,
    pool_name: Option<String>,
) -> Result<String, String> {
    if let Some(id) = pool_id.filter(|s| !s.trim().is_empty()) {
        return Ok(id);
    }
    let Some(name) = pool_name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) else {
        return Err("hello requires poolId or poolName".to_string());
    };
    let ids = sqlx::query_scalar::<_, String>(
        "SELECT id FROM ms_resource_pool WHERE name = $1 AND enabled AND NOT deleted",
    )
    .bind(&name)
    .fetch_all(db)
    .await
    .map_err(|_| "pool lookup failed (storage error)".to_string())?;
    match ids.as_slice() {
        [id] => Ok(id.clone()),
        [] => Err(format!("unknown or disabled pool name: {name}")),
        many => Err(format!("ambiguous pool name: {name} matches {} pools", many.len())),
    }
}

async fn runner_session(mut socket: WebSocket, hub: Arc<PoolHub>, db: migrate::PgPool) {
    // First frame must be a hello within 10s.
    let hello = tokio::time::timeout(Duration::from_secs(10), socket.recv()).await;
    let (hello_pool_id, hello_pool_name, name, max_concurrent, capabilities) = match hello {
        Ok(Some(Ok(Message::Text(t)))) => match serde_json::from_str::<RunnerMsg>(&t) {
            Ok(RunnerMsg::Hello { pool_id, pool_name, name, max_concurrent, capabilities }) => {
                (pool_id, pool_name, name, max_concurrent, capabilities)
            }
            _ => {
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
        },
        _ => return,
    };
    let pool_id = match resolve_hello_pool(&db, hello_pool_id, hello_pool_name).await {
        Ok(id) => id,
        Err(reason) => {
            tracing::warn!(runner = %name, %reason, "pool runner hello rejected");
            let _ = socket
                .send(Message::Close(Some(CloseFrame {
                    code: close_code::POLICY,
                    reason: reason.into(),
                })))
                .await;
            return;
        }
    };
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerMsg>();
    let runner_id = hub.register(&pool_id, &name, max_concurrent as usize, capabilities, out_tx);
    let ack = ServerMsg::HelloAck { runner_id: runner_id.clone() };
    if let Ok(s) = serde_json::to_string(&ack) {
        let _ = socket.send(Message::Text(s.into())).await;
    }

    let mut heartbeat = tokio::time::interval(HEARTBEAT_EVERY);
    let mut last_seen = Instant::now();
    loop {
        tokio::select! {
            out = out_rx.recv() => {
                let Some(msg) = out else { break };
                let Ok(s) = serde_json::to_string(&msg) else { continue };
                if socket.send(Message::Text(s.into())).await.is_err() {
                    break;
                }
            }
            _ = heartbeat.tick() => {
                if last_seen.elapsed() > LIVENESS_TIMEOUT {
                    tracing::warn!(runner = %runner_id, "pool runner heartbeat timeout");
                    break;
                }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
            frame = socket.recv() => {
                let Some(Ok(frame)) = frame else { break };
                last_seen = Instant::now();
                match frame {
                    Message::Text(t) => {
                        if let Ok(msg) = serde_json::from_str::<RunnerMsg>(&t) {
                            hub.ingest(msg).await;
                        }
                    }
                    Message::Close(_) => break,
                    // Pong / Ping (axum auto-pongs) only refresh liveness.
                    _ => {}
                }
            }
        }
    }
    hub.unregister(&runner_id).await;
}

/// Online runner count per pool (in-memory registry; complements the pool CRUD
/// DTO without touching its storage schema).
async fn runner_status(
    _user: webauth::AuthUser,
    State(st): State<WsState>,
) -> Json<HashMap<String, usize>> {
    Json(st.hub.online_counts())
}

/// Per-pool connected runner details (name / cap / in-flight) for the pool
/// detail page's executor-connection panel and the run-target indicators.
async fn runner_status_detail(
    _user: webauth::AuthUser,
    State(st): State<WsState>,
) -> Json<HashMap<String, Vec<RunnerStatus>>> {
    Json(st.hub.online_detail())
}

impl axum::extract::FromRef<WsState> for Arc<dyn SessionStore> {
    fn from_ref(s: &WsState) -> Self {
        s.sessions.clone()
    }
}

async fn run_events_ws(
    ws: WebSocketUpgrade,
    State(st): State<WsState>,
    Query(q): Query<WsQuery>,
    headers: HeaderMap,
) -> Response {
    if authorize(&st, &headers, &q).await.is_none() {
        return (StatusCode::UNAUTHORIZED, "invalid or missing token").into_response();
    }
    let Some(run_id) = q.run_id.clone().filter(|s| !s.trim().is_empty()) else {
        return (StatusCode::BAD_REQUEST, "runId required").into_response();
    };
    let events = st.hub.events();
    ws.on_upgrade(move |socket| relay_run_events(socket, events, run_id))
}

/// Replays history then streams live events; closes after runComplete.
async fn relay_run_events(mut socket: WebSocket, events: Arc<RunEventHub>, run_id: String) {
    let (history, mut rx) = events.subscribe(&run_id);
    let mut complete = false;
    for line in history {
        complete |= line.contains("\"runComplete\"");
        if socket.send(Message::Text(line.into())).await.is_err() {
            return;
        }
    }
    if complete {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    let mut keepalive = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            ev = rx.recv() => {
                match ev {
                    Ok(line) => {
                        let done = line.contains("\"runComplete\"");
                        if socket.send(Message::Text(line.into())).await.is_err() {
                            return;
                        }
                        if done {
                            let _ = socket.send(Message::Close(None)).await;
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => {
                        let _ = socket.send(Message::Close(None)).await;
                        return;
                    }
                }
            }
            _ = keepalive.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    return;
                }
            }
            frame = socket.recv() => {
                match frame {
                    None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pool: &str, caps: &[&str], max: usize) -> RunnerEntry {
        let (tx, _rx) = mpsc::unbounded_channel();
        RunnerEntry {
            pool_id: pool.to_string(),
            name: "r".to_string(),
            max_concurrent: max,
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            tx,
        }
    }

    fn state(runners: Vec<(&str, RunnerEntry)>) -> HubState {
        HubState {
            runners: runners.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            pending: HashMap::new(),
            queues: HashMap::new(),
            seq: 0,
        }
    }

    fn req(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_requirements_match_any_runner() {
        let e = entry("p", &[], 4);
        assert!(runner_eligible(&e, &req(&[])));
        let st = state(vec![("r1", entry("p", &[], 4))]);
        assert_eq!(pick_runner(&st, "p", &req(&[])).as_deref(), Some("r1"));
    }

    #[test]
    fn runner_must_advertise_every_required_tag() {
        let e = entry("p", &["http", "browser"], 4);
        assert!(runner_eligible(&e, &req(&["http"])));
        assert!(runner_eligible(&e, &req(&["http", "browser"])));
        assert!(!runner_eligible(&e, &req(&["http", "grpc"])));
        assert!(!runner_eligible(&e, &req(&["grpc"])));
    }

    #[test]
    fn pick_skips_runners_missing_required_tags() {
        let st = state(vec![("r1", entry("p", &[], 4)), ("r2", entry("p", &["x"], 4))]);
        assert_eq!(pick_runner(&st, "p", &req(&["x"])).as_deref(), Some("r2"));
        assert!(pick_runner(&st, "p", &req(&["y"])).is_none());
    }

    #[test]
    fn pick_respects_pool_and_capacity_among_eligible() {
        // r1 is capable but saturated (cap 1, one pending run); r2 lacks the tag.
        let mut st = state(vec![("r1", entry("p", &["x"], 1)), ("r2", entry("p", &[], 4))]);
        st.pending.insert(
            "run-1".to_string(),
            Pending {
                runner_id: "r1".to_string(),
                ctx: RunCtx {
                    scenario_id: String::new(),
                    project_id: String::new(),
                    case_count: 0,
                    report_id: "run-1".to_string(),
                    finalize_report: true,
                },
                done: None,
            },
        );
        assert!(pick_runner(&st, "p", &req(&["x"])).is_none());
        // Without requirements the idle runner wins.
        assert_eq!(pick_runner(&st, "p", &req(&[])).as_deref(), Some("r2"));
        // Other pools never match.
        assert!(pick_runner(&st, "other", &req(&[])).is_none());
    }
}
