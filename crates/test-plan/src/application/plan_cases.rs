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

    pub async fn link(&self, plan_id: &str, case_id: &str, name: &str) -> Result<(), RepoError> {
        self.repo.link_case(plan_id, case_id, name).await
    }

    /// 返回 false 表示该用例未挂入计划(调用方据此 404)。
    pub async fn record(
        &self,
        plan_id: &str,
        case_id: &str,
        status: CaseStatus,
        result: Option<CaseResult>,
    ) -> Result<bool, RepoError> {
        self.repo.record_result(plan_id, case_id, status, result.as_ref()).await
    }

    pub async fn list(&self, plan_id: &str) -> Result<Vec<PlanCase>, RepoError> {
        self.repo.list_cases(plan_id).await
    }
}
