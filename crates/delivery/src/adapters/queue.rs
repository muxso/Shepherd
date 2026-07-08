//! runtime 无公网入站,故派发是 runtime 出站长轮询认领(pull),不是 server→runtime 推。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use webauth::{AuthUser, SessionStore};

use crate::domain::ExecutorKind;
use crate::ports::{
    AgentExecutor, Claimed, DispatchOutcome, EventSink, ExecError, FleetRegistry, QueueStat,
    RuntimeInfo, WorkQueue, WorkSpec,
};

const KNOWN_CAPS: [ExecutorKind; 4] = ExecutorKind::ALL;

#[derive(Default)]
pub struct InMemoryWorkQueue {
    inner: Mutex<VecDeque<WorkSpec>>,
}

impl InMemoryWorkQueue {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn try_claim(&self, caps: &[ExecutorKind]) -> Option<WorkSpec> {
        let mut q = self.inner.lock().expect("lock");
        let pos = q.iter().position(|s| caps.contains(&s.executor))?;
        q.remove(pos)
    }
}

#[async_trait]
impl WorkQueue for InMemoryWorkQueue {
    async fn enqueue(&self, spec: &WorkSpec) {
        self.inner.lock().expect("lock").push_back(spec.clone());
    }

    // 认领即从队列移除,故无 PEL:ack 与超时回收都无需操作,consumer 被忽略。
    async fn claim(&self, caps: &[ExecutorKind], wait: Duration, _consumer: &str) -> Option<Claimed> {
        let step = Duration::from_millis(300);
        let mut waited = Duration::ZERO;
        loop {
            if let Some(spec) = self.try_claim(caps) {
                return Some(Claimed { spec });
            }
            if waited >= wait {
                return None;
            }
            tokio::time::sleep(step).await;
            waited += step;
        }
    }

    async fn ack(&self, _attempt_id: &str) {
        // 认领即移除,无待处理列表,no-op。
    }

    // 无 PEL,故 in_flight / oldest 恒 0;ready = 各能力排队数。
    async fn stats(&self) -> Vec<QueueStat> {
        let q = self.inner.lock().expect("lock");
        KNOWN_CAPS
            .into_iter()
            .map(|k| QueueStat {
                executor: k,
                ready: q.iter().filter(|s| s.executor == k).count() as u64,
                in_flight: 0,
                oldest_in_flight_ms: 0,
            })
            .collect()
    }
}

pub struct QueueAgentExecutor {
    queue: Arc<dyn WorkQueue>,
}

impl QueueAgentExecutor {
    pub fn new(queue: Arc<dyn WorkQueue>) -> Self {
        Self { queue }
    }
}

#[async_trait]
impl AgentExecutor for QueueAgentExecutor {
    async fn dispatch(
        &self,
        spec: &WorkSpec,
        _sink: &dyn EventSink,
    ) -> Result<DispatchOutcome, ExecError> {
        // 仅入队,不在此执行;事件经 runtime 的 HTTP 回调回流,sink 在此用不到。
        self.queue.enqueue(spec).await;
        Ok(DispatchOutcome::Accepted { run_id: spec.attempt_id.clone() })
    }
}

#[derive(Clone)]
struct FleetState {
    queue: Arc<dyn WorkQueue>,
    registry: Arc<dyn FleetRegistry>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<FleetState> for Arc<dyn SessionStore> {
    fn from_ref(s: &FleetState) -> Self {
        s.sessions.clone()
    }
}

/// `queue` 必须与 `QueueAgentExecutor` 共享同一实例,否则认领端点拿不到入队任务。
pub fn router(
    queue: Arc<dyn WorkQueue>,
    registry: Arc<dyn FleetRegistry>,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/agent/work/claim", get(claim))
        .route("/agent/work/stats", get(stats))
        .route("/agent/runtime", post(register).get(list_runtimes))
        .route("/agent/runtime/{id}/heartbeat", post(heartbeat))
        .with_state(FleetState { queue, registry, sessions })
}

#[derive(Deserialize)]
struct ClaimQuery {
    #[serde(default)]
    caps: Option<String>,
    #[serde(default)]
    runtime: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkSpecDto {
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

impl From<WorkSpec> for WorkSpecDto {
    fn from(s: WorkSpec) -> Self {
        Self {
            attempt_id: s.attempt_id,
            decomposition_id: s.decomposition_id,
            task_id: s.task_id,
            title: s.title,
            description: s.description,
            acceptance_criteria: s.acceptance_criteria,
            executor: s.executor.as_str().to_string(),
            context: s.context,
            instructions: s.instructions,
        }
    }
}

fn parse_caps(raw: &Option<String>) -> Vec<ExecutorKind> {
    match raw {
        Some(s) if !s.trim().is_empty() => {
            s.split(',').filter_map(|c| ExecutorKind::parse(c.trim())).collect()
        }
        _ => KNOWN_CAPS.to_vec(),
    }
}

async fn claim(
    user: AuthUser,
    State(st): State<FleetState>,
    Query(q): Query<ClaimQuery>,
) -> Response {
    if !user.can("DELIVERY", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let caps = parse_caps(&q.caps);
    if caps.is_empty() {
        return (StatusCode::BAD_REQUEST, "no known caps").into_response();
    }
    let consumer = q.runtime.as_deref().filter(|s| !s.is_empty()).unwrap_or("anon");
    match st.queue.claim(&caps, Duration::from_secs(20), consumer).await {
        Some(c) => (StatusCode::OK, Json(WorkSpecDto::from(c.spec))).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueStatDto {
    executor: String,
    ready: u64,
    in_flight: u64,
    oldest_in_flight_ms: u64,
}

impl From<QueueStat> for QueueStatDto {
    fn from(s: QueueStat) -> Self {
        Self {
            executor: s.executor.as_str().to_string(),
            ready: s.ready,
            in_flight: s.in_flight,
            oldest_in_flight_ms: s.oldest_in_flight_ms,
        }
    }
}

async fn stats(State(st): State<FleetState>) -> Response {
    let stats: Vec<QueueStatDto> =
        st.queue.stats().await.into_iter().map(QueueStatDto::from).collect();
    (StatusCode::OK, Json(stats)).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterBody {
    name: String,
    #[serde(default)]
    caps: Vec<String>,
    #[serde(default)]
    max_concurrency: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RegisteredResponse {
    runtime_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeResponse {
    id: String,
    name: String,
    caps: Vec<String>,
    max_concurrency: u32,
    last_seen_ms: u64,
    online: bool,
}

impl From<RuntimeInfo> for RuntimeResponse {
    fn from(r: RuntimeInfo) -> Self {
        Self {
            id: r.id,
            name: r.name,
            caps: r.caps,
            max_concurrency: r.max_concurrency,
            last_seen_ms: r.last_seen_ms,
            online: r.online,
        }
    }
}

async fn register(
    user: AuthUser,
    State(st): State<FleetState>,
    Json(b): Json<RegisterBody>,
) -> Response {
    if !user.can("DELIVERY", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    if b.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "name required").into_response();
    }
    let id = st.registry.register(b.name.trim(), &b.caps, b.max_concurrency.unwrap_or(1)).await;
    (StatusCode::CREATED, Json(RegisteredResponse { runtime_id: id })).into_response()
}

// 未知 id 返回 404,runtime 据此触发重新 register(回收后的重新上线路径)。
async fn heartbeat(
    user: AuthUser,
    State(st): State<FleetState>,
    Path(id): Path<String>,
) -> Response {
    if !user.can("DELIVERY", "UPDATE") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    if st.registry.heartbeat(&id).await {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (StatusCode::NOT_FOUND, "unknown runtime").into_response()
    }
}

async fn list_runtimes(State(st): State<FleetState>) -> Response {
    let list: Vec<RuntimeResponse> =
        st.registry.list().await.into_iter().map(RuntimeResponse::from).collect();
    (StatusCode::OK, Json(list)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str, kind: ExecutorKind) -> WorkSpec {
        WorkSpec {
            attempt_id: id.into(),
            decomposition_id: "d1".into(),
            task_id: "t1".into(),
            title: "build".into(),
            description: "do it".into(),
            acceptance_criteria: vec![],
            executor: kind,
            context: None,
            instructions: None,
        }
    }

    struct NoopSink;
    #[async_trait]
    impl EventSink for NoopSink {
        async fn emit(&self, _e: crate::domain::NewExecutionEvent) {}
    }

    #[tokio::test]
    async fn dispatch_enqueues_and_accepts() {
        let q = InMemoryWorkQueue::new();
        let ex = QueueAgentExecutor::new(q.clone());
        let out = ex.dispatch(&spec("a1", ExecutorKind::ClaudeCode), &NoopSink).await.expect("dispatch");
        assert_eq!(out, DispatchOutcome::Accepted { run_id: "a1".into() });
        assert_eq!(q.len(), 1);
    }

    #[tokio::test]
    async fn claim_pops_matching_capability_only() {
        let q = InMemoryWorkQueue::new();
        q.enqueue(&spec("a1", ExecutorKind::Codex)).await;
        assert!(q.claim(&[ExecutorKind::ClaudeCode], Duration::from_millis(50), "rt-1").await.is_none());
        let got = q.claim(&[ExecutorKind::Codex], Duration::from_millis(50), "rt-1").await.expect("claim");
        assert_eq!(got.spec.attempt_id, "a1");
        assert!(q.is_empty());
    }

    #[tokio::test]
    async fn stats_count_ready_per_capability() {
        let q = InMemoryWorkQueue::new();
        q.enqueue(&spec("a1", ExecutorKind::ClaudeCode)).await;
        q.enqueue(&spec("a2", ExecutorKind::ClaudeCode)).await;
        q.enqueue(&spec("b1", ExecutorKind::Codex)).await;
        let stats = q.stats().await;
        let by = |k: ExecutorKind| stats.iter().find(|s| s.executor == k).expect("cap present");
        assert_eq!(by(ExecutorKind::ClaudeCode).ready, 2);
        assert_eq!(by(ExecutorKind::Codex).ready, 1);
        assert_eq!(by(ExecutorKind::OpenCode).ready, 0);
        assert!(stats.iter().all(|s| s.in_flight == 0 && s.oldest_in_flight_ms == 0));
        q.claim(&[ExecutorKind::ClaudeCode], Duration::from_millis(50), "rt-1").await.expect("claim");
        assert_eq!(by_cap(&q.stats().await, ExecutorKind::ClaudeCode).ready, 1);
    }

    fn by_cap(stats: &[QueueStat], k: ExecutorKind) -> QueueStat {
        stats.iter().find(|s| s.executor == k).expect("cap present").clone()
    }

    #[test]
    fn parse_caps_defaults_to_all_known() {
        assert_eq!(parse_caps(&None), KNOWN_CAPS.to_vec());
        assert_eq!(parse_caps(&Some("CLAUDE_CODE".into())), vec![ExecutorKind::ClaudeCode]);
        assert_eq!(parse_caps(&Some("  CODEX , bogus ".into())), vec![ExecutorKind::Codex]);
    }
}
