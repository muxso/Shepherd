use std::sync::Arc;

use crate::domain::{status_by_children, CaseCounts, ExecStatus, PlanType};
use crate::ports::{PlanRepository, RepoError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanStatistics {
    pub status: ExecStatus,
    pub total: u64,
    pub pass_rate: f64,
    pub execute_rate: f64,
    pub is_pass: bool,
}

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanStatisticsError {
    #[error("plan not found")]
    PlanNotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct PlanStatisticsUseCase {
    repo: Arc<dyn PlanRepository>,
}

impl PlanStatisticsUseCase {
    pub fn new(repo: Arc<dyn PlanRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, plan_id: &str) -> Result<PlanStatistics, PlanStatisticsError> {
        let plan = self.repo.get(plan_id).await?.ok_or(PlanStatisticsError::PlanNotFound)?;
        let threshold = self.repo.pass_threshold(plan_id).await?;

        match plan.plan_type {
            PlanType::Plan => {
                let counts = self.repo.case_counts(plan_id).await?;
                Ok(Self::stats(counts, counts.derive_status(plan.archived), threshold))
            }
            PlanType::Group => {
                let children = self.repo.children(plan_id).await?;
                let mut total = CaseCounts::default();
                let mut child_statuses = Vec::with_capacity(children.len());
                for child in &children {
                    let c = self.repo.case_counts(&child.id).await?;
                    child_statuses.push(c.derive_status(child.archived));
                    total = total.add(&c);
                }
                // Group status must be derived from child statuses, not from the summed counts.
                let status = if plan.archived {
                    ExecStatus::Archived
                } else {
                    status_by_children(&child_statuses)
                };
                Ok(Self::stats(total, status, threshold))
            }
        }
    }

    pub async fn with_name(
        &self,
        plan_id: &str,
    ) -> Result<(String, PlanStatistics), PlanStatisticsError> {
        let plan = self.repo.get(plan_id).await?.ok_or(PlanStatisticsError::PlanNotFound)?;
        let stats = self.execute(plan_id).await?;
        Ok((plan.name, stats))
    }

    fn stats(counts: CaseCounts, status: ExecStatus, threshold: f64) -> PlanStatistics {
        PlanStatistics {
            status,
            total: counts.total(),
            pass_rate: counts.pass_rate(),
            execute_rate: counts.execute_rate(),
            is_pass: counts.is_pass(threshold),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryPlanRepository;
    use crate::domain::{ExecStatus::*, NewPlan, ROOT_GROUP};

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[tokio::test]
    async fn plan_statistics_from_own_counts() {
        let repo = InMemoryPlanRepository::new();
        let plan =
            repo.seed(NewPlan::new("proj1", "冒烟", PlanType::Plan, ROOT_GROUP).expect("v")).await;
        repo.set_counts(
            &plan.id,
            CaseCounts { pending: 1, success: 2, error: 1, ..Default::default() },
        );
        repo.set_threshold(&plan.id, 0.5);

        let uc = PlanStatisticsUseCase::new(Arc::new(repo));
        let s = uc.execute(&plan.id).await.expect("ok");
        assert_eq!(s.status, Underway);
        assert_eq!(s.total, 4);
        assert!(approx(s.pass_rate, 0.5));
        assert!(approx(s.execute_rate, 0.75));
        assert!(s.is_pass);
    }

    #[tokio::test]
    async fn group_aggregates_children_counts_and_status() {
        let repo = InMemoryPlanRepository::new();
        let group =
            repo.seed(NewPlan::new("proj1", "组", PlanType::Group, ROOT_GROUP).expect("v")).await;
        let a = repo.seed(NewPlan::new("proj1", "A", PlanType::Plan, &group.id).expect("v")).await;
        let b = repo.seed(NewPlan::new("proj1", "B", PlanType::Plan, &group.id).expect("v")).await;
        repo.set_counts(&a.id, CaseCounts { success: 2, ..Default::default() });
        repo.set_counts(&b.id, CaseCounts { pending: 1, success: 1, ..Default::default() });
        repo.set_threshold(&group.id, 0.5);

        let uc = PlanStatisticsUseCase::new(Arc::new(repo));
        let s = uc.execute(&group.id).await.expect("ok");
        assert_eq!(s.status, Underway);
        assert_eq!(s.total, 4);
        assert!(approx(s.pass_rate, 0.75));
    }

    #[tokio::test]
    async fn empty_group_is_prepared() {
        let repo = InMemoryPlanRepository::new();
        let group =
            repo.seed(NewPlan::new("proj1", "空组", PlanType::Group, ROOT_GROUP).expect("v")).await;
        repo.set_threshold(&group.id, 0.5);
        let uc = PlanStatisticsUseCase::new(Arc::new(repo));
        let s = uc.execute(&group.id).await.expect("ok");
        assert_eq!(s.status, Prepared);
        assert_eq!(s.total, 0);
    }

    #[tokio::test]
    async fn missing_plan_is_not_found() {
        let uc = PlanStatisticsUseCase::new(Arc::new(InMemoryPlanRepository::new()));
        assert_eq!(uc.execute("ghost").await, Err(PlanStatisticsError::PlanNotFound));
    }
}
