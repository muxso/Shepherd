use std::collections::BTreeMap;

use async_trait::async_trait;

use crate::domain::{Bug, BugRelation, NewBug, RelationKind, StatusFlowGraph};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait BugRepository: Send + Sync {
    async fn status_flow(&self, project_id: &str) -> Result<StatusFlowGraph, RepoError>;

    async fn insert(&self, new_bug: &NewBug, initial_status: &str) -> Result<Bug, RepoError>;

    async fn list(&self, project_id: &str) -> Result<Vec<Bug>, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<Bug>, RepoError>;

    /// Sets the status and stamps updated_by/updated_at with the operator.
    async fn set_status(
        &self,
        id: &str,
        status: &str,
        operator: Option<&str>,
    ) -> Result<(), RepoError>;

    /// Updates title/severity/handler (severity/handler are full replacements,
    /// None clears) and stamps updated_by/updated_at. Returns the updated bug,
    /// or None when it doesn't exist.
    async fn update_meta(
        &self,
        id: &str,
        title: &str,
        severity: Option<&str>,
        handler: Option<&str>,
        operator: Option<&str>,
    ) -> Result<Option<Bug>, RepoError>;

    /// Replaces custom field values wholesale (empty map clears all) and stamps
    /// updated_by/updated_at with the operator.
    async fn set_custom_fields(
        &self,
        id: &str,
        fields: &BTreeMap<String, String>,
        operator: Option<&str>,
    ) -> Result<(), RepoError>;

    async fn add_follower(&self, bug_id: &str, user_id: &str) -> Result<(), RepoError>;

    async fn remove_follower(&self, bug_id: &str, user_id: &str) -> Result<(), RepoError>;

    async fn list_followers(&self, bug_id: &str) -> Result<Vec<String>, RepoError>;

    async fn add_relation(&self, rel: &BugRelation) -> Result<(), RepoError>;

    async fn remove_relation(&self, rel: &BugRelation) -> Result<(), RepoError>;

    async fn list_relations(&self, bug_id: &str) -> Result<Vec<BugRelation>, RepoError>;

    /// Reverse lookup: bugs linked to a target entity (newest first, deleted excluded).
    async fn list_bugs_by_relation(
        &self,
        kind: RelationKind,
        target_id: &str,
    ) -> Result<Vec<Bug>, RepoError>;
}
