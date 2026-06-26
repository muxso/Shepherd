use async_trait::async_trait;

use crate::domain::{ReviewRecord, ReviewSetting, ReviewStatus};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSummary {
    pub id: String,
    pub pass_rule: String,
    pub reviewer_count: usize,
    pub total: usize,
    pub passed: usize,
    pub created_at: String,
    pub status: String,
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

    async fn create_review(
        &self,
        _project_id: &str,
        _pass_rule: &str,
        _reviewer_count: usize,
        _case_ids: &[String],
    ) -> Result<String, RepoError> {
        Err(RepoError::Backend("create_review unsupported".into()))
    }

    async fn list_reviews(&self, _project_id: &str) -> Result<Vec<ReviewSummary>, RepoError> {
        Ok(Vec::new())
    }

    async fn get_review(&self, _review_id: &str) -> Result<ReviewDetail, RepoError> {
        Err(RepoError::NotFound)
    }
}
