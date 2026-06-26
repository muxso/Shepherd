use async_trait::async_trait;

use crate::domain::{CaseCounts, CaseResult, CaseStatus, NewPlan, Plan, PlanCase};

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

    async fn children(&self, group_id: &str) -> Result<Vec<Plan>, RepoError>;

    async fn case_counts(&self, plan_id: &str) -> Result<CaseCounts, RepoError>;

    /// 通过阈值范围 0..=1。
    async fn pass_threshold(&self, plan_id: &str) -> Result<f64, RepoError>;

    /// 幂等;已存在则保留原状态,初始状态 PENDING。
    async fn link_case(&self, plan_id: &str, case_id: &str, name: &str) -> Result<(), RepoError>;

    async fn record_result(
        &self,
        plan_id: &str,
        case_id: &str,
        status: CaseStatus,
        result: Option<&CaseResult>,
    ) -> Result<bool, RepoError>;

    async fn list_cases(&self, plan_id: &str) -> Result<Vec<PlanCase>, RepoError>;

    /// 仅未归档计划。
    async fn list(&self, project_id: &str) -> Result<Vec<Plan>, RepoError>;

    async fn rename(&self, id: &str, name: &str) -> Result<bool, RepoError>;

    /// 连带删除挂入的用例。
    async fn delete(&self, id: &str) -> Result<bool, RepoError>;
}
