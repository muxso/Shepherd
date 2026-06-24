//! 计划仓储端口。提供计划读写、子计划查询、用例计数与通过阈值。

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

    /// 某计划组下的直接子计划。
    async fn children(&self, group_id: &str) -> Result<Vec<Plan>, RepoError>;

    /// 某(子)计划的用例执行计数(由挂入的用例状态聚合)。
    async fn case_counts(&self, plan_id: &str) -> Result<CaseCounts, RepoError>;

    /// 某计划的通过阈值(0..=1)。
    async fn pass_threshold(&self, plan_id: &str) -> Result<f64, RepoError>;

    /// 把一条用例挂入计划(幂等;已存在则保留原状态)。初始状态 PENDING。
    async fn link_case(&self, plan_id: &str, case_id: &str, name: &str) -> Result<(), RepoError>;

    /// 回写某用例在计划内的执行结果(状态 + 明细)。返回是否命中已挂入的用例。
    async fn record_result(
        &self,
        plan_id: &str,
        case_id: &str,
        status: CaseStatus,
        result: Option<&CaseResult>,
    ) -> Result<bool, RepoError>;

    /// 列出计划内的用例(含状态与执行明细),供报告逐条渲染。
    async fn list_cases(&self, plan_id: &str) -> Result<Vec<PlanCase>, RepoError>;

    /// 列出某项目下未归档的全部计划(供列表页;后端权威来源)。
    async fn list(&self, project_id: &str) -> Result<Vec<Plan>, RepoError>;

    /// 重命名计划。返回是否命中。
    async fn rename(&self, id: &str, name: &str) -> Result<bool, RepoError>;

    /// 删除计划及其挂入的用例。返回是否命中。
    async fn delete(&self, id: &str) -> Result<bool, RepoError>;
}
