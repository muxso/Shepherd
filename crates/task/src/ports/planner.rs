//! 拆分规划器出站端口:据需求规格(标题/描述/验收标准)提出任务拆分草案。
//! 可由启发式规则或 LLM 实现;返回的任务须按拓扑序(依赖在前,deps 用索引引用更早的任务)。

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanError {
    #[error("planner backend error: {0}")]
    Backend(String),
}

/// 待拆分的需求规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementSpec {
    pub requirement_id: String,
    pub requirement_version: u32,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
}

/// 规划出的一个任务。`dependencies` 是同一草案内更早任务的索引(0-based,须 < 自身索引)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTask {
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
    pub dependencies: Vec<usize>,
}

#[async_trait]
pub trait Planner: Send + Sync {
    async fn plan(&self, spec: &RequirementSpec) -> Result<Vec<PlannedTask>, PlanError>;
}
