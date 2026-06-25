//! 执行者机群(PoC):进程内工作队列 + 长轮询认领端点。
//!
//! 远程 AI 执行者(Claude Code / Codex / OpenCode)跑在内网、**无公网入站**,
//! 因此派发不能 server→runtime 推,而是 runtime **出站**长轮询认领:
//! - `QueueAgentExecutor::dispatch` 把 WorkSpec 入队,立即返回 `Accepted`(尝试置 Running);
//! - runtime 经 `GET /agent/work/claim?caps=CLAUDE_CODE` 长轮询拿到 WorkSpec,本地跑 claude,
//!   再经现成的 `/delivery/{id}/events|complete|fail` 回调收尾。
//!
//! PoC 边界:单进程内存队列,**无持久化 / 无注册 / 无心跳 / 无超时回收**
//! (见 `docs/remote-agent-runtime-plan.md` 阶段 2)。

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
    AgentExecutor, Claimed, DispatchOutcome, EventSink, ExecError, FleetRegistry, RuntimeInfo,
    WorkQueue, WorkSpec,
};

/// 进程内工作队列(单机/测试):`enqueue` 入队,runtime 长轮询认领。
/// 分布式请用 `RedisStreamQueue`(同一 `WorkQueue` 端口)。
#[derive(Default)]
pub struct InMemoryWorkQueue {
    inner: Mutex<VecDeque<WorkSpec>>,
}

impl InMemoryWorkQueue {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 当前队列深度(排障/测试用)。
    pub fn len(&self) -> usize {
        self.inner.lock().expect("lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 弹出第一个能力匹配的任务(无则 None)。
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

    /// 轮询直到 `wait` 超时(长轮询语义)。认领即移除,故 `ack`/回收均无需操作(`consumer` 忽略)。
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
        // 内存队列认领即移除,无待处理列表,no-op。
    }
}

/// 把任务入队、立即返回 `Accepted`(run_id = attempt_id);真正执行由某台 runtime
/// 认领后异步回调 `/delivery/{id}/complete` 收尾。
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
        // 仅入队,不在此执行;事件经 runtime 的 HTTP 回调回流,而非这里的 sink。
        self.queue.enqueue(spec).await;
        Ok(DispatchOutcome::Accepted { run_id: spec.attempt_id.clone() })
    }
}

// ---- HTTP:runtime 长轮询认领 ----

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

/// 挂载机群端点。`queue` 须与 `QueueAgentExecutor` 共享同一实例。
pub fn router(
    queue: Arc<dyn WorkQueue>,
    registry: Arc<dyn FleetRegistry>,
    sessions: Arc<dyn SessionStore>,
) -> Router {
    Router::new()
        .route("/agent/work/claim", get(claim))
        .route("/agent/runtime", post(register).get(list_runtimes))
        .route("/agent/runtime/{id}/heartbeat", post(heartbeat))
        .with_state(FleetState { queue, registry, sessions })
}

#[derive(Deserialize)]
struct ClaimQuery {
    /// 逗号分隔的能力,如 `CLAUDE_CODE,CODEX`;缺省 = 全部已知执行者。
    #[serde(default)]
    caps: Option<String>,
    /// 认领方 runtime id(用于 PEL 归属 + 死 runtime 回收);缺省回退匿名。
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
        _ => vec![ExecutorKind::ClaudeCode, ExecutorKind::Codex, ExecutorKind::OpenCode],
    }
}

/// 长轮询认领一个任务。有 → 200 WorkSpec;无(超时)→ 204。
/// 鉴权:与回调同级,需 `DELIVERY:UPDATE`。
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

// ---- HTTP:runtime 注册 / 心跳 / 列表(机群视图) ----

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

/// 登记一台 runtime(出站 register)。需 `DELIVERY:UPDATE`。
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

/// 心跳续约(出站 heartbeat)。未知 id → 404(runtime 据此重新 register)。
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

/// 机群视图:列出全部 runtime(含在线判定)。读端点开放。
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
        // 只认领 CLAUDE_CODE → 跳过 Codex 任务,短超时返回 None。
        assert!(q.claim(&[ExecutorKind::ClaudeCode], Duration::from_millis(50), "rt-1").await.is_none());
        // 认领 Codex → 拿到,队列清空。
        let got = q.claim(&[ExecutorKind::Codex], Duration::from_millis(50), "rt-1").await.expect("claim");
        assert_eq!(got.spec.attempt_id, "a1");
        assert!(q.is_empty());
    }

    #[test]
    fn parse_caps_defaults_to_all_known() {
        assert_eq!(
            parse_caps(&None),
            vec![ExecutorKind::ClaudeCode, ExecutorKind::Codex, ExecutorKind::OpenCode]
        );
        assert_eq!(parse_caps(&Some("CLAUDE_CODE".into())), vec![ExecutorKind::ClaudeCode]);
        assert_eq!(parse_caps(&Some("  CODEX , bogus ".into())), vec![ExecutorKind::Codex]);
    }
}
