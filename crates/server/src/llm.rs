//! 真实 LLM 接入(OpenAI 兼容 chat completions):一个 `LlmClient` + planner/judge/executor 三个适配器。
//! 各适配器构造任务专属提示,调 LLM,解析其 JSON 输出为对应端口类型。
//!
//! 环境:`SHEPHERD_LLM_URL`(如 .../v1/chat/completions)、`SHEPHERD_LLM_API_KEY`(可选)、
//! `SHEPHERD_LLM_MODEL`(默认 gpt-4o-mini)。client 用 no_proxy(与本仓其它 HTTP 适配器一致)。

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use delivery::domain::{Deliverable, DeliverableKind, EventKind, NewExecutionEvent};
use delivery::ports::{AgentExecutor, DispatchOutcome, EventSink, ExecError, WorkSpec};
use orchestrator::ports::{DeliverableView, Judge, Verdict};
use task::ports::{PlanError, PlannedTask, Planner, RequirementSpec};

/// 从 LLM 文本里抠出第一个 JSON 块(容忍模型外裹的散文/``` 围栏)。
fn extract_json(s: &str) -> &str {
    let start = s.find(['{', '[']);
    let end = s.rfind(['}', ']']);
    match (start, end) {
        (Some(a), Some(b)) if b >= a => &s[a..=b],
        _ => s.trim(),
    }
}

#[derive(Clone)]
pub struct LlmClient {
    client: reqwest::Client,
    url: String,
    api_key: Option<String>,
    model: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}
#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}
#[derive(Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: String,
}

impl LlmClient {
    pub fn new(url: impl Into<String>, api_key: Option<String>, model: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, url: url.into(), api_key, model: model.into() }
    }

    /// 从环境构造(未设 SHEPHERD_LLM_URL → None)。
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("SHEPHERD_LLM_URL").ok().filter(|u| !u.trim().is_empty())?;
        let api_key = std::env::var("SHEPHERD_LLM_API_KEY").ok().filter(|k| !k.trim().is_empty());
        let model = std::env::var("SHEPHERD_LLM_MODEL")
            .ok()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| "gpt-4o-mini".into());
        Some(Self::new(url, api_key, model))
    }

    async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        let body = json!({
            "model": self.model,
            "temperature": 0,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ]
        });
        let mut rb = self.client.post(&self.url).json(&body);
        if let Some(k) = &self.api_key {
            rb = rb.bearer_auth(k);
        }
        let resp = rb.send().await.map_err(|e| format!("LLM 不可达: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("LLM HTTP {}", resp.status()));
        }
        let parsed: ChatResponse = resp.json().await.map_err(|e| format!("LLM 响应解析失败: {e}"))?;
        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| "LLM 无 choices".to_string())
    }
}

// ===== Planner =====

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
            spec.acceptance_criteria.iter().map(|c| format!("- {c}")).collect::<Vec<_>>().join("\n")
        );
        let text = self.client.complete(PLANNER_SYSTEM, &user).await.map_err(PlanError::Backend)?;
        let dtos: Vec<PlannedTaskDto> = serde_json::from_str(extract_json(&text))
            .map_err(|e| PlanError::Backend(format!("规划结果解析失败: {e}")))?;
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

// ===== Judge =====

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
        match self.client.complete(JUDGE_SYSTEM, &user).await {
            Ok(text) => match serde_json::from_str::<VerdictDto>(extract_json(&text)) {
                Ok(v) => Verdict { passed: v.passed, reason: v.reason },
                Err(e) => Verdict { passed: false, reason: format!("裁决解析失败: {e}") },
            },
            // fail-closed:LLM 出错视为不通过。
            Err(e) => Verdict { passed: false, reason: e },
        }
    }
}

// ===== Executor =====

const EXECUTOR_SYSTEM: &str = "你是编码执行者。依据任务(及行为规范)产出变更摘要与一个引用(分支/PR)。\
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
        let text = self.client.complete(EXECUTOR_SYSTEM, &user).await.map_err(ExecError::Backend)?;
        let d: DeliverableDto = serde_json::from_str(extract_json(&text))
            .map_err(|e| ExecError::Backend(format!("交付物解析失败: {e}")))?;
        let reference =
            if d.reference.is_empty() { format!("llm://{}", spec.task_id) } else { d.reference };
        if let Ok(ev) = NewExecutionEvent::new(EventKind::Decision, "LLM 执行者产出交付物", Some(&d.summary)) {
            sink.emit(ev).await;
        }
        Ok(DispatchOutcome::Completed {
            deliverable: Deliverable { kind: DeliverableKind::Diff, reference, summary: d.summary },
        })
    }
}

// ===== 按环境组装(SHEPHERD_LLM_URL 在,才返回 Some) =====

pub fn planner() -> Option<Arc<dyn Planner>> {
    LlmClient::from_env().map(|c| Arc::new(LlmPlanner::new(c)) as Arc<dyn Planner>)
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
    use axum::{routing::post, Json, Router};
    use delivery::domain::ExecutorKind;
    use delivery::ports::NoopEventSink;

    // 启一个 OpenAI 兼容桩:把固定 content 包进 choices[0].message.content。
    async fn serve_llm(content: &'static str) -> String {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move || async move {
                Json(json!({ "choices": [ { "message": { "content": content } } ] }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}/v1/chat/completions")
    }

    fn spec() -> RequirementSpec {
        RequirementSpec {
            requirement_id: "r1".into(),
            requirement_version: 1,
            title: "登录".into(),
            description: "邮箱登录".into(),
            acceptance_criteria: vec!["登录成功".into()],
        }
    }

    #[tokio::test]
    async fn llm_planner_parses_tasks_even_with_prose() {
        // 模型外裹散文 + ``` 围栏,extract_json 仍能抠出数组
        let url = serve_llm("好的,这是计划:\n```json\n[{\"title\":\"实现登录\",\"acceptanceCriteria\":[\"登录成功\"],\"dependencies\":[]}]\n```").await;
        let p = LlmPlanner::new(LlmClient::new(url, None, "m"));
        let tasks = p.plan(&spec()).await.expect("plan");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "实现登录");
    }

    #[tokio::test]
    async fn llm_judge_parses_verdict() {
        let url = serve_llm("{\"passed\": false, \"reason\": \"缺测试\"}").await;
        let j = LlmJudge::new(LlmClient::new(url, None, "m"));
        let v = j.judge(&["登录成功".into()], &DeliverableView { kind: "DIFF".into(), reference: "b".into(), summary: "s".into() }).await;
        assert!(!v.passed);
        assert_eq!(v.reason, "缺测试");
    }

    #[tokio::test]
    async fn llm_executor_parses_deliverable_and_emits_event() {
        let url = serve_llm("{\"reference\":\"branch:feat\",\"summary\":\"实现完成\"}").await;
        let e = LlmExecutor::new(LlmClient::new(url, None, "m"));
        let ws = WorkSpec {
            decomposition_id: "d".into(),
            task_id: "t1".into(),
            title: "x".into(),
            description: "".into(),
            acceptance_criteria: vec![],
            executor: ExecutorKind::ClaudeCode,
            context: None,
            instructions: None,
        };
        match e.dispatch(&ws, &NoopEventSink).await.expect("dispatch") {
            DispatchOutcome::Completed { deliverable } => {
                assert_eq!(deliverable.reference, "branch:feat");
                assert_eq!(deliverable.summary, "实现完成");
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn llm_judge_fail_closed_on_bad_json() {
        let url = serve_llm("not json at all").await;
        let j = LlmJudge::new(LlmClient::new(url, None, "m"));
        let v = j.judge(&[], &DeliverableView { kind: "DIFF".into(), reference: "b".into(), summary: "s".into() }).await;
        assert!(!v.passed); // 解析失败 → 不通过
    }
}
