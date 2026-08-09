//! LLM adapters: pluggable implementations for the three AI touchpoints
//! (decomposition / execution / verification).
//!
//! Two wire protocols: OpenAI-compatible `chat/completions` (default) and native
//! Anthropic Messages (`SHEPHERD_LLM_WIRE=anthropic`, default model claude-opus-4-8).
//! Production hardening: per-call timeout, backoff retries on 429/5xx and network
//! errors (honoring `Retry-After`), plus latency/token recording for cost observability.
//!
//! Env vars: SHEPHERD_LLM_URL, SHEPHERD_LLM_API_KEY, SHEPHERD_LLM_MODEL,
//! SHEPHERD_LLM_WIRE (openai|anthropic), SHEPHERD_LLM_MAX_TOKENS (default 4096),
//! SHEPHERD_LLM_MAX_RETRIES (default 3), SHEPHERD_LLM_TIMEOUT_SECS (default 120).

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use delivery::domain::{Deliverable, DeliverableKind, EventKind, NewExecutionEvent};
use delivery::ports::{AgentExecutor, DispatchOutcome, EventSink, ExecError, WorkSpec};
use orchestrator::ports::{DeliverableView, Judge, Verdict};
use task::ports::{PlanError, PlannedTask, Planner, RequirementSpec};

fn extract_json(s: &str) -> &str {
    let start = s.find(['{', '[']);
    let end = s.rfind(['}', ']']);
    match (start, end) {
        (Some(a), Some(b)) if b >= a => &s[a..=b],
        _ => s.trim(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Wire {
    OpenAi,
    Anthropic,
}

#[derive(Clone)]
pub struct LlmClient {
    client: reqwest::Client,
    url: String,
    api_key: Option<String>,
    model: String,
    wire: Wire,
    max_tokens: u32,
    max_retries: u32,
}

#[derive(Default)]
struct Usage {
    input: u64,
    output: u64,
}

enum SendErr {
    Retryable { msg: String, retry_after: Option<Duration> },
    Fatal(String),
}

impl LlmClient {
    #[cfg(test)]
    fn new(url: impl Into<String>, api_key: Option<String>, model: impl Into<String>) -> Self {
        Self::build(url.into(), api_key, model.into(), Wire::OpenAi, 4096, 3, 120)
    }

    fn build(
        url: String,
        api_key: Option<String>,
        model: String,
        wire: Wire,
        max_tokens: u32,
        max_retries: u32,
        timeout_secs: u64,
    ) -> Self {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(timeout_secs.max(1)))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, url, api_key, model, wire, max_tokens, max_retries: max_retries.max(1) }
    }

    #[cfg(test)]
    fn with_wire(mut self, wire: Wire) -> Self {
        self.wire = wire;
        self
    }

    pub fn from_env() -> Option<Self> {
        let wire = match std::env::var("SHEPHERD_LLM_WIRE").ok().as_deref() {
            Some("anthropic") => Wire::Anthropic,
            _ => Wire::OpenAi,
        };
        let url = std::env::var("SHEPHERD_LLM_URL").ok().filter(|u| !u.trim().is_empty());
        let url = match (url, wire) {
            (Some(u), _) => u,
            (None, Wire::Anthropic) => "https://api.anthropic.com/v1/messages".to_string(),
            (None, Wire::OpenAi) => return None,
        };
        let api_key = std::env::var("SHEPHERD_LLM_API_KEY").ok().filter(|k| !k.trim().is_empty());
        let default_model = match wire {
            Wire::Anthropic => "claude-opus-4-8",
            Wire::OpenAi => "gpt-4o-mini",
        };
        let model = std::env::var("SHEPHERD_LLM_MODEL")
            .ok()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| default_model.to_string());
        let num = |k: &str, d: u32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let timeout = std::env::var("SHEPHERD_LLM_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120);
        Some(Self::build(
            url,
            api_key,
            model,
            wire,
            num("SHEPHERD_LLM_MAX_TOKENS", 4096),
            num("SHEPHERD_LLM_MAX_RETRIES", 3),
            timeout,
        ))
    }

    async fn complete(
        &self,
        prompt_version: &str,
        system: &str,
        user: &str,
    ) -> Result<String, String> {
        let started = Instant::now();
        let mut delay = Duration::from_millis(200);
        let mut last = String::new();
        for attempt in 1..=self.max_retries {
            match self.send_once(system, user).await {
                Ok((text, usage)) => {
                    tracing::info!(
                        wire = self.wire_name(), model = %self.model, prompt_version,
                        latency_ms = started.elapsed().as_millis() as u64,
                        input_tokens = usage.input, output_tokens = usage.output, attempt,
                        "LLM call completed"
                    );
                    return Ok(text);
                }
                Err(SendErr::Fatal(msg)) => return Err(msg),
                Err(SendErr::Retryable { msg, retry_after }) => {
                    last = msg;
                    if attempt == self.max_retries {
                        break;
                    }
                    let wait = retry_after.unwrap_or(delay);
                    tracing::warn!(attempt, "LLM retryable failure, retry after {wait:?}: {last}");
                    tokio::time::sleep(wait).await;
                    delay = (delay * 2).min(Duration::from_secs(20));
                }
            }
        }
        Err(format!("LLM still failing after {} retries: {last}", self.max_retries))
    }

    fn wire_name(&self) -> &'static str {
        match self.wire {
            Wire::OpenAi => "openai",
            Wire::Anthropic => "anthropic",
        }
    }

    async fn send_once(&self, system: &str, user: &str) -> Result<(String, Usage), SendErr> {
        let body = match self.wire {
            Wire::OpenAi => json!({
                "model": self.model,
                "temperature": 0,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": user }
                ]
            }),
            // Opus 4.x rejects `temperature`; `max_tokens` is required.
            Wire::Anthropic => json!({
                "model": self.model,
                "max_tokens": self.max_tokens,
                "system": system,
                "messages": [ { "role": "user", "content": user } ]
            }),
        };
        let mut rb = self.client.post(&self.url).json(&body);
        rb = match (self.wire, &self.api_key) {
            (Wire::Anthropic, key) => {
                rb = rb.header("anthropic-version", "2023-06-01");
                match key {
                    Some(k) => rb.header("x-api-key", k),
                    None => rb,
                }
            }
            (Wire::OpenAi, Some(k)) => rb.bearer_auth(k),
            (Wire::OpenAi, None) => rb,
        };

        let resp = rb.send().await.map_err(|e| SendErr::Retryable {
            msg: format!("LLM unreachable: {e}"),
            retry_after: None,
        })?;
        let status = resp.status();
        if status.is_success() {
            let val: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| SendErr::Fatal(format!("failed to parse the LLM response: {e}")))?;
            return self.parse_ok(&val).map_err(SendErr::Fatal);
        }
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let code = status.as_u16();
        let msg = format!("LLM HTTP {status}");
        if matches!(code, 429 | 500 | 502 | 503 | 504 | 529) {
            Err(SendErr::Retryable { msg, retry_after })
        } else {
            Err(SendErr::Fatal(msg))
        }
    }

    fn parse_ok(&self, val: &serde_json::Value) -> Result<(String, Usage), String> {
        match self.wire {
            Wire::OpenAi => {
                let text = val
                    .pointer("/choices/0/message/content")
                    .and_then(|c| c.as_str())
                    .ok_or_else(|| "LLM response has no choices".to_string())?
                    .to_string();
                let u = val.get("usage");
                let usage = Usage {
                    input: u
                        .and_then(|u| u.get("prompt_tokens"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                    output: u
                        .and_then(|u| u.get("completion_tokens"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                };
                Ok((text, usage))
            }
            Wire::Anthropic => {
                // The safety classifier may refuse: on stop_reason=refusal, content is empty, so treat it as an error.
                if val.get("stop_reason").and_then(|s| s.as_str()) == Some("refusal") {
                    return Err("LLM declined to answer (refusal)".to_string());
                }
                let text = val
                    .get("content")
                    .and_then(|c| c.as_array())
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                            .collect::<String>()
                    })
                    .unwrap_or_default();
                if text.is_empty() {
                    return Err("LLM response has no content".to_string());
                }
                let u = val.get("usage");
                let usage = Usage {
                    input: u
                        .and_then(|u| u.get("input_tokens"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                    output: u
                        .and_then(|u| u.get("output_tokens"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                };
                Ok((text, usage))
            }
        }
    }
}

const PLANNER_PROMPT_V: &str = "planner-v1";
const PLANNER_SYSTEM: &str = "你是软件任务规划器。把需求拆成可独立交付的任务,按拓扑序输出。\
只输出 JSON 数组,每项 {\"title\":string,\"description\":string,\"acceptanceCriteria\":[string],\"dependencies\":[int]};\
dependencies 为更早任务的下标(从 0 起,必须小于自身下标)。不要输出 JSON 以外的任何内容。";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlannedTaskDto {
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
    #[serde(default)]
    dependencies: Vec<usize>,
}

pub struct LlmPlanner {
    client: LlmClient,
}
impl LlmPlanner {
    pub fn new(client: LlmClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Planner for LlmPlanner {
    async fn plan(&self, spec: &RequirementSpec) -> Result<Vec<PlannedTask>, PlanError> {
        let user = format!(
            "需求标题: {}\n描述: {}\n验收标准:\n{}",
            spec.title,
            spec.description,
            spec.acceptance_criteria
                .iter()
                .map(|c| format!("- {c}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let text = self
            .client
            .complete(PLANNER_PROMPT_V, PLANNER_SYSTEM, &user)
            .await
            .map_err(PlanError::Backend)?;
        let dtos: Vec<PlannedTaskDto> = serde_json::from_str(extract_json(&text))
            .map_err(|e| PlanError::Backend(format!("failed to parse the plan result: {e}")))?;
        Ok(dtos
            .into_iter()
            .map(|d| PlannedTask {
                title: d.title,
                description: d.description,
                acceptance_criteria: d.acceptance_criteria,
                dependencies: d.dependencies,
            })
            .collect())
    }
}

const PRD_PROMPT_V: &str = "prd-v1";
const PRD_SYSTEM: &str = "你是资深产品经理,把用户粘贴的原始素材(MRD/会议纪要/想法)整理成结构化需求。\
只输出 JSON {\"title\":string,\"description\":string,\"acceptanceCriteria\":string[],\"priority\":string}。\
title 一句话;description 含背景/目标/范围;acceptanceCriteria 每条独立可判定(3~8 条);\
priority 取 P0/P1/P2/P3;不要输出 JSON 以外的任何内容。";

/// MRD → PRD drafting result.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrdDraft {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub priority: String,
}

pub struct LlmPrdDrafter {
    client: LlmClient,
}
impl LlmPrdDrafter {
    pub fn new(client: LlmClient) -> Self {
        Self { client }
    }

    pub async fn draft(&self, raw: &str) -> Result<PrdDraft, String> {
        let text = self.client.complete(PRD_PROMPT_V, PRD_SYSTEM, raw).await?;
        let mut d: PrdDraft = serde_json::from_str(extract_json(&text))
            .map_err(|e| format!("failed to parse the PRD draft: {e}"))?;
        d.title = d.title.trim().to_string();
        d.acceptance_criteria.retain(|c| !c.trim().is_empty());
        if d.title.is_empty() {
            return Err("PRD draft is missing a title".to_string());
        }
        Ok(d)
    }
}

const CASES_PROMPT_V: &str = "cases-v1";
const CASES_SYSTEM: &str = "你是资深测试工程师,基于需求与拆分任务设计功能测试用例。\
只输出 JSON 数组,每项 {\"name\":string,\"criterionIndexes\":number[],\"steps\":[{\"step\":string,\"expected\":string}]}。\
criterionIndexes 引用需求验收标准下标(0 起);每个任务至少 1 条用例;步骤要可执行、预期可判定;\
不要输出 JSON 以外的任何内容。";

pub struct LlmCaseDrafter {
    client: LlmClient,
}
impl LlmCaseDrafter {
    pub fn new(client: LlmClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl crate::case_drafter::CaseDrafter for LlmCaseDrafter {
    async fn draft(
        &self,
        spec: &RequirementSpec,
        tasks: &[task::domain::Task],
    ) -> Result<Vec<crate::case_drafter::DraftedCase>, String> {
        let criteria = spec
            .acceptance_criteria
            .iter()
            .enumerate()
            .map(|(i, c)| format!("[{i}] {c}"))
            .collect::<Vec<_>>()
            .join("\n");
        let task_list = tasks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                format!(
                    "[{i}] {}\n  描述: {}\n  任务验收: {}",
                    t.title,
                    t.description,
                    t.acceptance_criteria.join("; ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let user = format!(
            "需求标题: {}\n描述: {}\n需求验收标准:\n{}\n\n拆分任务:\n{}",
            spec.title, spec.description, criteria, task_list
        );
        let text = self.client.complete(CASES_PROMPT_V, CASES_SYSTEM, &user).await?;
        crate::case_drafter::parse_drafted(
            extract_json(&text),
            tasks.len(),
            spec.acceptance_criteria.len(),
        )
    }
}

const JUDGE_PROMPT_V: &str = "judge-v1";
const JUDGE_SYSTEM: &str = "你是严格的验收评审。依据验收标准评判交付物是否达标。\
只输出 JSON {\"passed\":bool,\"reason\":string}。不要输出 JSON 以外的任何内容。";

#[derive(Deserialize)]
struct VerdictDto {
    passed: bool,
    #[serde(default)]
    reason: String,
}

pub struct LlmJudge {
    client: LlmClient,
}
impl LlmJudge {
    pub fn new(client: LlmClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Judge for LlmJudge {
    async fn judge(&self, criteria: &[String], deliverable: &DeliverableView) -> Verdict {
        let user = format!(
            "验收标准:\n{}\n\n交付物:\n类型: {}\n引用: {}\n摘要: {}",
            criteria.iter().map(|c| format!("- {c}")).collect::<Vec<_>>().join("\n"),
            deliverable.kind,
            deliverable.reference,
            deliverable.summary
        );
        match self.client.complete(JUDGE_PROMPT_V, JUDGE_SYSTEM, &user).await {
            Ok(text) => match serde_json::from_str::<VerdictDto>(extract_json(&text)) {
                Ok(v) => Verdict { passed: v.passed, reason: v.reason },
                Err(e) => {
                    Verdict { passed: false, reason: format!("failed to parse verdict: {e}") }
                }
            },
            // Fail closed: an LLM error counts as not passed.
            Err(e) => Verdict { passed: false, reason: e },
        }
    }
}

const EXECUTOR_PROMPT_V: &str = "executor-v1";
const EXECUTOR_SYSTEM: &str =
    "你是编码执行者。依据任务(及行为规范)产出变更摘要与一个引用(分支/PR)。\
只输出 JSON {\"reference\":string,\"summary\":string}。不要输出 JSON 以外的任何内容。";

#[derive(Deserialize)]
struct DeliverableDto {
    #[serde(default)]
    reference: String,
    #[serde(default)]
    summary: String,
}

pub struct LlmExecutor {
    client: LlmClient,
}
impl LlmExecutor {
    pub fn new(client: LlmClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl AgentExecutor for LlmExecutor {
    async fn dispatch(
        &self,
        spec: &WorkSpec,
        sink: &dyn EventSink,
    ) -> Result<DispatchOutcome, ExecError> {
        let mut user = String::new();
        if let Some(instr) = &spec.instructions {
            user.push_str(&format!("行为规范:\n{instr}\n\n"));
        }
        user.push_str(&format!("任务: {}\n{}\n", spec.title, spec.description));
        if !spec.acceptance_criteria.is_empty() {
            user.push_str("验收标准:\n");
            for c in &spec.acceptance_criteria {
                user.push_str(&format!("- {c}\n"));
            }
        }
        let text = self
            .client
            .complete(EXECUTOR_PROMPT_V, EXECUTOR_SYSTEM, &user)
            .await
            .map_err(ExecError::Backend)?;
        let d: DeliverableDto = serde_json::from_str(extract_json(&text))
            .map_err(|e| ExecError::Backend(format!("failed to parse the deliverable: {e}")))?;
        let reference =
            if d.reference.is_empty() { format!("llm://{}", spec.task_id) } else { d.reference };
        if let Ok(ev) = NewExecutionEvent::new(
            EventKind::Decision,
            "LLM executor produced a deliverable",
            Some(&d.summary),
        ) {
            sink.emit(ev).await;
        }
        Ok(DispatchOutcome::Completed {
            deliverable: Deliverable { kind: DeliverableKind::Diff, reference, summary: d.summary },
        })
    }
}

pub fn planner() -> Option<Arc<dyn Planner>> {
    LlmClient::from_env().map(|c| Arc::new(LlmPlanner::new(c)) as Arc<dyn Planner>)
}
pub fn prd_drafter() -> Option<Arc<LlmPrdDrafter>> {
    LlmClient::from_env().map(|c| Arc::new(LlmPrdDrafter::new(c)))
}

pub fn case_drafter() -> Option<Arc<dyn crate::case_drafter::CaseDrafter>> {
    LlmClient::from_env()
        .map(|c| Arc::new(LlmCaseDrafter::new(c)) as Arc<dyn crate::case_drafter::CaseDrafter>)
}

pub fn judge() -> Option<Arc<dyn Judge>> {
    LlmClient::from_env().map(|c| Arc::new(LlmJudge::new(c)) as Arc<dyn Judge>)
}
pub fn executor() -> Option<Arc<dyn AgentExecutor>> {
    LlmClient::from_env().map(|c| Arc::new(LlmExecutor::new(c)) as Arc<dyn AgentExecutor>)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::{routing::post, Json, Router};
    use delivery::domain::ExecutorKind;
    use delivery::ports::NoopEventSink;
    use std::sync::atomic::{AtomicU32, Ordering};

    async fn spawn(app: Router, path: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}{path}")
    }

    async fn serve_openai(content: &'static str) -> String {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move || async move {
                Json(json!({
                    "choices": [ { "message": { "content": content } } ],
                    "usage": { "prompt_tokens": 7, "completion_tokens": 3 }
                }))
            }),
        );
        spawn(app, "/v1/chat/completions").await
    }

    async fn serve_anthropic(text: &'static str) -> String {
        let app = Router::new().route(
            "/v1/messages",
            post(move || async move {
                Json(json!({
                    "content": [ { "type": "text", "text": text } ],
                    "usage": { "input_tokens": 11, "output_tokens": 5 },
                    "stop_reason": "end_turn"
                }))
            }),
        );
        spawn(app, "/v1/messages").await
    }

    fn spec() -> RequirementSpec {
        RequirementSpec {
            requirement_id: "r1".into(),
            requirement_version: 1,
            title: "login".into(),
            description: "email login".into(),
            acceptance_criteria: vec!["login success".into()],
        }
    }

    #[tokio::test]
    async fn llm_planner_parses_tasks_even_with_prose() {
        let url = serve_openai("Sure, here is the plan:\n```json\n[{\"title\":\"implement login\",\"acceptanceCriteria\":[\"login success\"],\"dependencies\":[]}]\n```").await;
        let p = LlmPlanner::new(LlmClient::new(url, None, "m"));
        let tasks = p.plan(&spec()).await.expect("plan");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "implement login");
    }

    #[tokio::test]
    async fn llm_judge_parses_verdict() {
        let url = serve_openai("{\"passed\": false, \"reason\": \"missing tests\"}").await;
        let j = LlmJudge::new(LlmClient::new(url, None, "m"));
        let v = j
            .judge(
                &["login success".into()],
                &DeliverableView {
                    kind: "DIFF".into(),
                    reference: "b".into(),
                    summary: "s".into(),
                },
            )
            .await;
        assert!(!v.passed);
        assert_eq!(v.reason, "missing tests");
    }

    #[tokio::test]
    async fn llm_executor_parses_deliverable_and_emits_event() {
        let url =
            serve_openai("{\"reference\":\"branch:feat\",\"summary\":\"implementation done\"}")
                .await;
        let e = LlmExecutor::new(LlmClient::new(url, None, "m"));
        let ws = WorkSpec {
            attempt_id: "a".into(),
            decomposition_id: "d".into(),
            task_id: "t1".into(),
            title: "x".into(),
            description: "".into(),
            acceptance_criteria: vec![],
            executor: ExecutorKind::ClaudeCode,
            context: None,
            instructions: None,
            target_runtime: None,
        };
        match e.dispatch(&ws, &NoopEventSink).await.expect("dispatch") {
            DispatchOutcome::Completed { deliverable } => {
                assert_eq!(deliverable.reference, "branch:feat");
                assert_eq!(deliverable.summary, "implementation done");
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn llm_judge_fail_closed_on_bad_json() {
        let url = serve_openai("not json at all").await;
        let j = LlmJudge::new(LlmClient::new(url, None, "m"));
        let v = j
            .judge(
                &[],
                &DeliverableView {
                    kind: "DIFF".into(),
                    reference: "b".into(),
                    summary: "s".into(),
                },
            )
            .await;
        assert!(!v.passed);
    }

    #[tokio::test]
    async fn anthropic_wire_parses_content_blocks() {
        let url = serve_anthropic(
            "[{\"title\":\"implement login\",\"acceptanceCriteria\":[],\"dependencies\":[]}]",
        )
        .await;
        let p = LlmPlanner::new(
            LlmClient::new(url, Some("k".into()), "claude-opus-4-8").with_wire(Wire::Anthropic),
        );
        let tasks = p.plan(&spec()).await.expect("plan");
        assert_eq!(tasks[0].title, "implement login");
    }

    #[tokio::test]
    async fn anthropic_refusal_is_fail_closed() {
        let app = Router::new().route(
            "/v1/messages",
            post(|| async { Json(json!({ "content": [], "stop_reason": "refusal" })) }),
        );
        let url = spawn(app, "/v1/messages").await;
        let j = LlmJudge::new(
            LlmClient::new(url, Some("k".into()), "claude-opus-4-8").with_wire(Wire::Anthropic),
        );
        let v = j
            .judge(
                &[],
                &DeliverableView {
                    kind: "DIFF".into(),
                    reference: "b".into(),
                    summary: "s".into(),
                },
            )
            .await;
        assert!(!v.passed);
        assert!(v.reason.contains("refusal"), "got: {}", v.reason);
    }

    #[tokio::test]
    async fn retries_on_503_then_succeeds() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move || {
                let c = c.clone();
                async move {
                    if c.fetch_add(1, Ordering::SeqCst) == 0 {
                        return (StatusCode::SERVICE_UNAVAILABLE, "busy").into_response();
                    }
                    Json(json!({ "choices": [ { "message": { "content": "{\"passed\":true,\"reason\":\"ok\"}" } } ] }))
                        .into_response()
                }
            }),
        );
        let url = spawn(app, "/v1/chat/completions").await;
        let j = LlmJudge::new(LlmClient::new(url, None, "m"));
        let v = j
            .judge(
                &[],
                &DeliverableView {
                    kind: "DIFF".into(),
                    reference: "b".into(),
                    summary: "s".into(),
                },
            )
            .await;
        assert!(v.passed, "should have retried past the 503: {}", v.reason);
        assert_eq!(calls.load(Ordering::SeqCst), 2, "one failure + one success");
    }

    #[tokio::test]
    async fn non_retryable_4xx_fails_fast() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    StatusCode::BAD_REQUEST
                }
            }),
        );
        let url = spawn(app, "/v1/chat/completions").await;
        let p = LlmPlanner::new(LlmClient::new(url, None, "m"));
        assert!(p.plan(&spec()).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1, "400 must not be retried");
    }
}
