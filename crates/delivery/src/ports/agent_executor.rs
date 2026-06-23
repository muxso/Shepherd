//! 执行者出站端口:把一份工作规格交给某种 AI 执行者(Claude Code / Codex)。
//!
//! 复用 api-test `DispatchOutcome` 的二态语义,但承载更丰富的载荷(交付物/run_id):
//! 实现可为本地子进程(同步 → `Completed`)或远端 Agent API(异步 → `Accepted`)。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{Deliverable, ExecutorKind};
use crate::ports::EventSink;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExecError {
    #[error("executor backend error: {0}")]
    Backend(String),
}

/// 交给执行者的工作规格(由任务派生)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkSpec {
    /// 本次交付尝试 id —— 异步执行者据此回调 `/delivery/{attempt_id}/complete`。
    pub attempt_id: String,
    pub decomposition_id: String,
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
    pub executor: ExecutorKind,
    /// 可选的仓库/分支等上下文提示。
    pub context: Option<String>,
    /// 可选的 AI 行为规范(由 skill 组合而来,注入执行者提示以规范行为)。
    pub instructions: Option<String>,
}

/// 下发结果(据此推进交付尝试状态):
/// - `Accepted { run_id }`:远端**异步**接单 → 尝试置 Running,等回调收尾;
/// - `Completed { deliverable }`:**同步**跑完 → 尝试直接置 Delivered。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    Accepted { run_id: String },
    Completed { deliverable: Deliverable },
}

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    /// 派发工作给执行者;运行中产生的执行事件经 `sink` 实时回流(供审计)。
    async fn dispatch(
        &self,
        spec: &WorkSpec,
        sink: &dyn EventSink,
    ) -> Result<DispatchOutcome, ExecError>;
}
