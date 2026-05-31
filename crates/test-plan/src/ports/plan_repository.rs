//! 计划仓储端口。提供计划读写、子计划查询、用例计数与通过阈值。

use async_trait::async_trait;

use crate::domain::{CaseCounts, NewPlan, Plan};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait PlanRepository: Send + Sync {
    async fn insert(&self, new_plan: &NewPlan) -> Result<Plan, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<Plan>, RepoError>;

    /// 某计划组下的直接子计划。
    async fn children(&self, group_id: &str) -> Result<Vec<Plan>, RepoError>;

    /// 某(子)计划的用例执行计数。
    async fn case_counts(&self, plan_id: &str) -> Result<CaseCounts, RepoError>;

    /// 某计划的通过阈值(0..=1)。
    async fn pass_threshold(&self, plan_id: &str) -> Result<f64, RepoError>;
}
