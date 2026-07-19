use async_trait::async_trait;

use crate::domain::{ReviewRecord, ReviewSetting, ReviewStatus};

use thiserror::Error;

/// Editable review header fields shown on the list page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReviewMeta {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub module_id: Option<String>,
    /// RFC3339/ISO timestamps as text; adapters cast to timestamptz.
    pub start_at: Option<String>,
    pub end_at: Option<String>,
}

/// Input for creating a review (setting + case set + header meta).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReview {
    pub project_id: String,
    pub pass_rule: String,
    pub reviewer_count: usize,
    pub case_ids: Vec<String>,
    pub created_by: String,
    pub meta: ReviewMeta,
}

/// Input for editing a review's header meta and pass setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewMetaUpdate {
    pub pass_rule: String,
    pub reviewer_count: usize,
    pub meta: ReviewMeta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSummary {
    pub id: String,
    pub pass_rule: String,
    pub reviewer_count: usize,
    pub total: usize,
    pub passed: usize,
    pub created_at: String,
    pub status: String,
    pub created_by: Option<String>,
    /// Distinct reviewer ids that submitted verdicts (from history).
    pub reviewers: Vec<String>,
    pub meta: ReviewMeta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewCaseStatus {
    pub case_id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewDetail {
    pub id: String,
    pub pass_rule: String,
    pub reviewer_count: usize,
    pub cases: Vec<ReviewCaseStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("review or case not found")]
    NotFound,
}

#[async_trait]
pub trait ReviewRepository: Send + Sync {
    async fn review_setting(&self, review_id: &str) -> Result<ReviewSetting, RepoError>;

    // Returns history in time-ascending order, as the aggregate expects.
    async fn history_of(
        &self,
        review_id: &str,
        case_id: &str,
    ) -> Result<Vec<ReviewRecord>, RepoError>;

    async fn append_history(
        &self,
        review_id: &str,
        case_id: &str,
        record: &ReviewRecord,
    ) -> Result<(), RepoError>;

    async fn set_case_status(
        &self,
        review_id: &str,
        case_id: &str,
        status: ReviewStatus,
    ) -> Result<(), RepoError>;

    async fn create_review(&self, _req: &NewReview) -> Result<String, RepoError> {
        Err(RepoError::Backend("create_review unsupported".into()))
    }

    async fn update_review_meta(
        &self,
        _review_id: &str,
        _update: &ReviewMetaUpdate,
    ) -> Result<(), RepoError> {
        Err(RepoError::Backend("update_review_meta unsupported".into()))
    }

    async fn list_reviews(&self, _project_id: &str) -> Result<Vec<ReviewSummary>, RepoError> {
        Ok(Vec::new())
    }

    /// Soft delete: hides the review from list/get; verdict history stays intact.
    async fn delete_review(&self, _review_id: &str) -> Result<(), RepoError> {
        Err(RepoError::Backend("delete_review unsupported".into()))
    }

    async fn get_review(&self, _review_id: &str) -> Result<ReviewDetail, RepoError> {
        Err(RepoError::NotFound)
    }
}
