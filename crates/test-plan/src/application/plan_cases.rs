//! 计划用例编排:把用例挂入计划 / 回写执行结果 / 列出(供报告逐条渲染)。

use std::sync::Arc;

use crate::domain::{CaseResult, CaseStatus, PlanCase};
use crate::ports::{PlanRepository, RepoError};

#[derive(Clone)]
pub struct PlanCaseUseCase {
    repo: Arc<dyn PlanRepository>,
}

impl PlanCaseUseCase {
    pub fn new(repo: Arc<dyn PlanRepository>) -> Self {
        Self { repo }
    }

    /// 把用例挂入计划(幂等)。
    pub async fn link(&self, plan_id: &str, case_id: &str, name: &str) -> Result<(), RepoError> {
        self.repo.link_case(plan_id, case_id, name).await
    }

    /// 回写执行结果。返回 false 表示该用例未挂入计划(调用方据此 404)。
    pub async fn record(
        &self,
        plan_id: &str,
        case_id: &str,
        status: CaseStatus,
        result: Option<CaseResult>,
    ) -> Result<bool, RepoError> {
        self.repo.record_result(plan_id, case_id, status, result.as_ref()).await
    }

    /// 列出计划内用例(含状态与执行明细)。
    pub async fn list(&self, plan_id: &str) -> Result<Vec<PlanCase>, RepoError> {
        self.repo.list_cases(plan_id).await
    }
}
