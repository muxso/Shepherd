use async_trait::async_trait;

use crate::domain::{Bug, BugRelation, NewBug, StatusFlowGraph};

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

    async fn set_status(&self, id: &str, status: &str) -> Result<(), RepoError>;

    async fn add_follower(&self, bug_id: &str, user_id: &str) -> Result<(), RepoError>;

    async fn remove_follower(&self, bug_id: &str, user_id: &str) -> Result<(), RepoError>;

    async fn list_followers(&self, bug_id: &str) -> Result<Vec<String>, RepoError>;

    async fn add_relation(&self, rel: &BugRelation) -> Result<(), RepoError>;

    async fn remove_relation(&self, rel: &BugRelation) -> Result<(), RepoError>;

    async fn list_relations(&self, bug_id: &str) -> Result<Vec<BugRelation>, RepoError>;
}
