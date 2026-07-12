//! Port contract: every `*_active` method only sees non-soft-deleted requirements.

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{ChangeEntry, NewChange, NewRequirement, Requirement, StageRow, StatusCounts};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait RequirementRepository: Send + Sync {
    async fn find_active_by_title(
        &self,
        project_id: &str,
        title: &str,
    ) -> Result<Option<Requirement>, RepoError>;

    async fn insert(&self, new: &NewRequirement) -> Result<Requirement, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<Requirement>, RepoError>;

    async fn count_active(&self, project_id: &str) -> Result<u64, RepoError>;

    async fn list_active(
        &self,
        project_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<Requirement>, RepoError>;

    /// Versions are immutable: save only appends versions not yet persisted, never rewrites existing ones.
    async fn save(&self, requirement: &Requirement) -> Result<(), RepoError>;

    /// Manual ordering: write explicit ranks (1..N) for these requirements in the given order; unlisted ones keep their rank.
    async fn set_order(&self, project_id: &str, ordered_ids: &[String]) -> Result<(), RepoError>;

    /// Per-status counts of non-deleted requirements in a project (dashboard).
    async fn status_counts(&self, project_id: &str) -> Result<StatusCounts, RepoError>;

    /// Direct children (non-soft-deleted), in display order.
    async fn children(&self, parent_id: &str) -> Result<Vec<Requirement>, RepoError>;

    /// Insert or overwrite one stage row (idempotent upsert keyed by (requirement, stage)).
    async fn upsert_stage(&self, requirement_id: &str, row: &StageRow) -> Result<(), RepoError>;

    /// The requirement's 7-stage pipeline, always all stages in order (missing rows filled with PENDING defaults).
    async fn stages(&self, requirement_id: &str) -> Result<Vec<StageRow>, RepoError>;

    /// Append a batch of change-log entries (append-only; timestamps stamped by the storage layer).
    async fn append_change(
        &self,
        requirement_id: &str,
        changes: &[NewChange],
    ) -> Result<(), RepoError>;

    /// Change log, newest first, at most `limit` entries.
    async fn list_changes(
        &self,
        requirement_id: &str,
        limit: u32,
    ) -> Result<Vec<ChangeEntry>, RepoError>;
}
