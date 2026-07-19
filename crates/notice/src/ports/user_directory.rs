use async_trait::async_trait;

use crate::ports::notice_store::RepoError;

/// Receiver resolution: maps mention names to user ids and lists project members.
#[async_trait]
pub trait NoticeUserDirectory: Send + Sync {
    /// Resolves mention candidates (username / display name / user id) to session
    /// user ids; unknown names are dropped.
    async fn resolve_user_ids(&self, names: &[String]) -> Result<Vec<String>, RepoError>;

    /// User ids of a project's members (empty when the project has none).
    async fn project_member_ids(&self, project_id: &str) -> Result<Vec<String>, RepoError>;
}
