//! Planner port: returned tasks must be in topological order with index-based back-references.

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanError {
    #[error("planner backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementSpec {
    pub requirement_id: String,
    pub requirement_version: u32,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
}

/// `dependencies` are 0-based indices of earlier tasks in the same plan (must be < own index).
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
