use async_trait::async_trait;

use crate::domain::{NewMember, ProjectMember};
use crate::ports::RepoError;

#[async_trait]
pub trait ProjectMemberRepository: Send + Sync {
    /// Adds a member or changes their role: updates `role` if (project_id, user_id) already exists.
    async fn upsert(&self, member: &NewMember) -> Result<ProjectMember, RepoError>;

    /// Lists project members ordered by join time.
    async fn list(&self, project_id: &str) -> Result<Vec<ProjectMember>, RepoError>;

    /// Removes a member; returns whether a row was actually removed.
    async fn remove(&self, project_id: &str, user_id: &str) -> Result<bool, RepoError>;
}
