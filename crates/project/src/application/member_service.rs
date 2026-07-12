use std::sync::Arc;

use thiserror::Error;

use crate::domain::{MemberError, NewMember, ProjectMember};
use crate::ports::{ProjectMemberRepository, RepoError};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MemberCmdError {
    #[error("validation: {0}")]
    Validation(#[from] MemberError),
    #[error("member not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct ProjectMemberService {
    repo: Arc<dyn ProjectMemberRepository>,
}

impl ProjectMemberService {
    pub fn new(repo: Arc<dyn ProjectMemberRepository>) -> Self {
        Self { repo }
    }

    /// Adds a member or changes their role. Validation lives in the domain layer (`NewMember`).
    pub async fn add(
        &self,
        project_id: &str,
        user_id: &str,
        role: &str,
    ) -> Result<ProjectMember, MemberCmdError> {
        let new = NewMember::new(project_id, user_id, role)?;
        Ok(self.repo.upsert(&new).await?)
    }

    pub async fn list(&self, project_id: &str) -> Result<Vec<ProjectMember>, MemberCmdError> {
        Ok(self.repo.list(project_id).await?)
    }

    /// Removes a member; returns `NotFound` if they are not a member.
    pub async fn remove(&self, project_id: &str, user_id: &str) -> Result<(), MemberCmdError> {
        if self.repo.remove(project_id, user_id).await? {
            Ok(())
        } else {
            Err(MemberCmdError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryProjectMemberRepository;
    use crate::domain::MemberRole;

    fn svc() -> ProjectMemberService {
        ProjectMemberService::new(Arc::new(InMemoryProjectMemberRepository::new()))
    }

    #[tokio::test]
    async fn add_then_list() {
        let s = svc();
        s.add("p1", "u1", "owner").await.expect("add");
        s.add("p1", "u2", "member").await.expect("add");
        let list = s.list("p1").await.expect("list");
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|m| m.user_id == "u1" && m.role == MemberRole::Owner));
    }

    #[tokio::test]
    async fn add_is_upsert_changes_role() {
        let s = svc();
        s.add("p1", "u1", "member").await.expect("add");
        let m = s.add("p1", "u1", "owner").await.expect("re-add");
        assert_eq!(m.role, MemberRole::Owner);
        assert_eq!(s.list("p1").await.expect("list").len(), 1);
    }

    #[tokio::test]
    async fn invalid_role_is_validation_error() {
        let s = svc();
        assert!(matches!(
            s.add("p1", "u1", "king").await,
            Err(MemberCmdError::Validation(MemberError::InvalidRole(_)))
        ));
    }

    #[tokio::test]
    async fn remove_then_not_found() {
        let s = svc();
        s.add("p1", "u1", "member").await.expect("add");
        s.remove("p1", "u1").await.expect("remove");
        assert_eq!(s.list("p1").await.expect("list").len(), 0);
        assert_eq!(s.remove("p1", "u1").await, Err(MemberCmdError::NotFound));
    }

    #[tokio::test]
    async fn list_is_scoped_per_project() {
        let s = svc();
        s.add("p1", "u1", "member").await.expect("add");
        s.add("p2", "u2", "member").await.expect("add");
        assert_eq!(s.list("p1").await.expect("list").len(), 1);
    }
}
