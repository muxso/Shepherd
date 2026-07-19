use async_trait::async_trait;

use crate::domain::{CaseCounts, CaseResult, CaseStatus, NewPlan, Plan, PlanCase, PlanMeta};

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

    /// Pass threshold in the range 0..=1.
    async fn pass_threshold(&self, plan_id: &str) -> Result<f64, RepoError>;

    /// Idempotent; an existing link keeps its status, otherwise starts as PENDING.
    async fn link_case(&self, plan_id: &str, case_id: &str, name: &str) -> Result<(), RepoError>;

    /// Removes the link (and its recorded result); false when the case is not linked.
    async fn unlink_case(&self, plan_id: &str, case_id: &str) -> Result<bool, RepoError>;

    async fn record_result(
        &self,
        plan_id: &str,
        case_id: &str,
        status: CaseStatus,
        result: Option<&CaseResult>,
    ) -> Result<bool, RepoError>;

    async fn list_cases(&self, plan_id: &str) -> Result<Vec<PlanCase>, RepoError>;

    /// Non-archived plans only, newest first, with editable meta for the list view.
    async fn list(&self, project_id: &str) -> Result<Vec<(Plan, PlanMeta)>, RepoError>;

    /// Archives (hides from the list) or restores a plan; false when missing.
    async fn set_archived(&self, id: &str, archived: bool) -> Result<bool, RepoError>;

    async fn rename(&self, id: &str, name: &str) -> Result<bool, RepoError>;

    /// Also deletes the linked cases.
    async fn delete(&self, id: &str) -> Result<bool, RepoError>;

    /// Full detail: core plan + editable meta + planning doc (mind-map, stored verbatim).
    async fn detail(
        &self,
        id: &str,
    ) -> Result<Option<(Plan, PlanMeta, Option<serde_json::Value>)>, RepoError>;

    /// Full-row write of name + editable meta; false when the plan is missing.
    async fn update_meta(&self, id: &str, name: &str, meta: &PlanMeta) -> Result<bool, RepoError>;

    /// Stores the planning doc verbatim; false when the plan is missing.
    async fn set_planning(&self, id: &str, doc: &serde_json::Value) -> Result<bool, RepoError>;
}
