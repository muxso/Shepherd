use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{FunctionalCase, NewFunctionalCase};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone)]
pub struct CoverageCase {
    pub criterion_index: i32,
    pub case_id: String,
    pub case_name: String,
    pub module: String,
    pub priority: String,
}

#[derive(Debug, Clone)]
pub struct CaseRequirement {
    pub requirement_id: String,
    pub requirement_title: String,
    pub criterion_index: i32,
}

/// One recorded field change of a case (audit trail entry).
#[derive(Debug, Clone)]
pub struct CaseChange {
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    pub actor: String,
    pub created_at: String,
}

/// Bug linked to a case via ms_bug_relation (kind = FUNCTIONAL_CASE).
#[derive(Debug, Clone)]
pub struct CaseBugRef {
    pub bug_id: String,
    pub title: String,
    pub status: String,
    pub created_by: String,
    pub handler: String,
}

/// Review containing the case, with the case's own review status.
#[derive(Debug, Clone)]
pub struct CaseReviewRef {
    pub review_id: String,
    pub status: String,
    pub created_at: String,
}

/// Test plan containing the case, with the case's execution outcome.
#[derive(Debug, Clone)]
pub struct CasePlanRef {
    pub plan_id: String,
    pub plan_name: String,
    pub project_name: String,
    pub archived: bool,
    pub exec_status: String,
    pub executed_at: String,
}

/// Pre/post case dependency edge, resolved to the other case's identity.
#[derive(Debug, Clone)]
pub struct CaseDependencyRef {
    pub case_id: String,
    pub num: i64,
    pub name: String,
    pub created_by: String,
}

#[async_trait]
pub trait CaseRepository: Send + Sync {
    async fn insert(&self, c: &NewFunctionalCase) -> Result<FunctionalCase, RepoError>;

    async fn update(
        &self,
        id: &str,
        c: &NewFunctionalCase,
    ) -> Result<Option<FunctionalCase>, RepoError>;

    async fn delete(&self, id: &str) -> Result<bool, RepoError>;

    async fn list_by_project(&self, project_id: &str) -> Result<Vec<FunctionalCase>, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<FunctionalCase>, RepoError>;

    async fn link_requirement_case(
        &self,
        requirement_id: &str,
        criterion_index: i32,
        functional_case_id: &str,
        project_id: &str,
    ) -> Result<(), RepoError>;

    async fn unlink_requirement_case(
        &self,
        requirement_id: &str,
        criterion_index: i32,
        functional_case_id: &str,
    ) -> Result<(), RepoError>;

    async fn cases_for_requirement(
        &self,
        requirement_id: &str,
    ) -> Result<Vec<CoverageCase>, RepoError>;

    async fn requirements_for_case(
        &self,
        functional_case_id: &str,
    ) -> Result<Vec<CaseRequirement>, RepoError>;

    /// Appends field-level audit entries; `changes` items are (field, old, new).
    async fn record_changes(
        &self,
        case_id: &str,
        changes: &[(String, String, String)],
        actor: &str,
    ) -> Result<(), RepoError>;

    async fn list_changes(&self, case_id: &str) -> Result<Vec<CaseChange>, RepoError>;

    async fn add_dependency(
        &self,
        project_id: &str,
        case_id: &str,
        depends_on_id: &str,
        created_by: &str,
    ) -> Result<(), RepoError>;

    async fn remove_dependency(&self, case_id: &str, depends_on_id: &str)
        -> Result<(), RepoError>;

    /// `reverse = false` lists preconditions of the case; `true` lists cases
    /// that depend on it (post cases).
    async fn dependencies_for_case(
        &self,
        case_id: &str,
        reverse: bool,
    ) -> Result<Vec<CaseDependencyRef>, RepoError>;

    async fn bugs_for_case(&self, case_id: &str) -> Result<Vec<CaseBugRef>, RepoError>;

    async fn reviews_for_case(&self, case_id: &str) -> Result<Vec<CaseReviewRef>, RepoError>;

    async fn plans_for_case(&self, case_id: &str) -> Result<Vec<CasePlanRef>, RepoError>;
}
