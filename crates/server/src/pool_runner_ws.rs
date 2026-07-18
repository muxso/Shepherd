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

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
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
    tx: mpsc::UnboundedSender<ServerMsg>,
}

/// Context needed to finalize a remote run (report status + execution record).
pub struct RunCtx {
    pub scenario_id: String,
    pub project_id: String,
    pub case_count: i32,
}

struct Pending {
    runner_id: String,
    ctx: RunCtx,
    done: Option<oneshot::Sender<String>>,
}

struct HubState {
    runners: HashMap<String, RunnerEntry>,
    pending: HashMap<String, Pending>,
    seq: u64,
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

    fn register(&self, pool_id: &str, name: &str, tx: mpsc::UnboundedSender<ServerMsg>) -> String {
        let mut st = self.lock();
        st.seq += 1;
        let id = format!("pr-{}-{}", st.seq, &uuid::Uuid::new_v4().to_string()[..8]);
        st.runners.insert(
            id.clone(),
            RunnerEntry { pool_id: pool_id.to_string(), name: name.to_string(), tx },
        );
        tracing::info!(runner = %id, pool = %pool_id, %name, "pool runner connected");
        id
    }

    /// Removes the runner and fails all of its in-flight runs.
    async fn unregister(&self, runner_id: &str) {
        let orphans: Vec<String> = {
            let mut st = self.lock();
            if st.runners.remove(runner_id).is_some() {
                tracing::info!(runner = %runner_id, "pool runner disconnected");
            }
            st.pending
                .iter()
                .filter(|(_, p)| p.runner_id == runner_id)
                .map(|(rid, _)| rid.clone())
                .collect()
        };
        for run_id in orphans {
            self.finish(&run_id, "ERROR", Some("runner disconnected")).await;
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

    /// Sends the compiled plan to the least-loaded runner of the pool and
    /// registers completion bookkeeping. None = no live runner (caller falls
    /// back to in-process execution).
    pub fn dispatch(
        &self,
        pool_id: &str,
        run_id: &str,
        nodes: Vec<WireNode>,
        env: WireEnv,
        stop_on_failure: bool,
        ctx: RunCtx,
    ) -> Option<oneshot::Receiver<String>> {
        let (done_tx, done_rx) = oneshot::channel();
        {
            let mut st = self.lock();
            // Least busy runner in the pool by in-flight run count.
            let busy: HashMap<String, usize> =
                st.pending.values().fold(HashMap::new(), |mut m, p| {
                    *m.entry(p.runner_id.clone()).or_default() += 1;
                    m
                });
            let (runner_id, entry) = st
                .runners
                .iter()
                .filter(|(_, r)| r.pool_id == pool_id)
                .min_by_key(|(id, _)| busy.get(*id).copied().unwrap_or(0))?;
            let msg = ServerMsg::Run { run_id: run_id.to_string(), nodes, env, stop_on_failure };
            if entry.tx.send(msg).is_err() {
                return None;
            }
            tracing::info!(run = %run_id, runner = %runner_id, name = %entry.name, pool = %pool_id, "scenario dispatched to pool runner");
            let runner_id = runner_id.clone();
            st.pending.insert(run_id.to_string(), Pending { runner_id, ctx, done: Some(done_tx) });
        }
        Some(done_rx)
    }

    /// Finalizes a remote run once: report status, execution record, completion
    /// event, waiter wake-up. Later duplicate calls are no-ops.
    async fn finish(&self, run_id: &str, status: &str, reason: Option<&str>) {
        let Some(mut p) = self.lock().pending.remove(run_id) else { return };
        if let Some(r) = reason {
            tracing::warn!(run = %run_id, %status, reason = r, "remote run finalized");
        }
        let _ = self.deps.reports.set_status(run_id, status).await;
        let _ = self
            .deps
            .recorder
            .execute(&p.ctx.scenario_id, &p.ctx.project_id, status, p.ctx.case_count, Some(run_id))
            .await;
        self.events.publish(
            run_id,
            serde_json::json!({"type": "runComplete", "runId": run_id, "status": status}),
        );
        if let Some(tx) = p.done.take() {
            let _ = tx.send(status.to_string());
        }
    }

    /// Abandons a remote run from the waiter side (dispatch timeout).
    pub async fn abort_run(&self, run_id: &str) {
        self.finish(run_id, "ERROR", Some("dispatch timeout")).await;
    }

    /// Ingests one event from a runner: persist through the local sinks and
    /// relay to subscribed browsers.
    async fn ingest(&self, msg: RunnerMsg) {
        match msg {
            RunnerMsg::Hello { .. } => {} // handled at connect
            RunnerMsg::StepStarted { run_id, step_id } => {
                self.events.publish(
                    &run_id,
                    serde_json::json!({"type": "stepStarted", "runId": run_id, "stepId": step_id}),
                );
            }
            RunnerMsg::StepFinished { run_id, step_id, outcome, failures } => {
                let _ = self.deps.sink.record(&run_id, &step_id, &outcome, &failures).await;
                self.events.publish(
                    &run_id,
                    serde_json::json!({
                        "type": "stepFinished", "runId": run_id, "stepId": step_id,
                        "status": outcome, "failures": failures,
                    }),
                );
            }
            RunnerMsg::StepDetail(d) => {
                let _ = self
                    .deps
                    .sink
                    .record_detail(
                        &d.run_id,
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
                    &d.run_id,
                    serde_json::json!({
                        "type": "stepDetail", "runId": d.run_id, "stepId": d.step_id,
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
}

#[derive(Deserialize)]
struct WsQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default, rename = "runId")]
    run_id: Option<String>,
}

pub fn router(hub: Arc<PoolHub>, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/api/pool-runner/ws", get(runner_ws))
        .route("/api/pool-runner/status", get(runner_status))
        .route("/api/run-events/ws", get(run_events_ws))
        .with_state(WsState { hub, sessions })
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
    ws.on_upgrade(move |socket| runner_session(socket, st.hub))
}

async fn runner_session(mut socket: WebSocket, hub: Arc<PoolHub>) {
    // First frame must be a hello within 10s.
    let hello = tokio::time::timeout(Duration::from_secs(10), socket.recv()).await;
    let (pool_id, name) = match hello {
        Ok(Some(Ok(Message::Text(t)))) => match serde_json::from_str::<RunnerMsg>(&t) {
            Ok(RunnerMsg::Hello { pool_id, name, .. }) => (pool_id, name),
            _ => {
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
        },
        _ => return,
    };
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerMsg>();
    let runner_id = hub.register(&pool_id, &name, out_tx);
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
