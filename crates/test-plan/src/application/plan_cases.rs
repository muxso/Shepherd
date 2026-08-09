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

    /// Returns false when the case is not linked to the plan (callers turn this into a 404).
    pub async fn record(
        &self,
        plan_id: &str,
        case_id: &str,
        status: CaseStatus,
        result: Option<CaseResult>,
    ) -> Result<bool, RepoError> {
        self.repo.record_result(plan_id, case_id, status, result.as_ref()).await
    }

    /// Returns false when the case is not linked to the plan (callers turn this into a 404).
    pub async fn unlink(&self, plan_id: &str, case_id: &str) -> Result<bool, RepoError> {
        self.repo.unlink_case(plan_id, case_id).await
    }

    pub async fn list(&self, plan_id: &str) -> Result<Vec<PlanCase>, RepoError> {
        self.repo.list_cases(plan_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryPlanRepository;

    fn uc() -> PlanCaseUseCase {
        PlanCaseUseCase::new(Arc::new(InMemoryPlanRepository::new()))
    }

    #[tokio::test]
    async fn unlink_removes_linked_case() {
        let uc = uc();
        uc.link("p1", "c1", "login").await.expect("link");
        uc.link("p1", "c2", "logout").await.expect("link");
        assert!(uc.unlink("p1", "c1").await.expect("unlink"));
        let left = uc.list("p1").await.expect("list");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].case_id, "c2");
    }

    #[tokio::test]
    async fn unlink_missing_case_returns_false() {
        let uc = uc();
        uc.link("p1", "c1", "login").await.expect("link");
        assert!(!uc.unlink("p1", "ghost").await.expect("unlink"));
        assert!(!uc.unlink("other-plan", "c1").await.expect("unlink"));
        assert_eq!(uc.list("p1").await.expect("list").len(), 1);
    }
}
