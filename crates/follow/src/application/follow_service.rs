use std::sync::Arc;

use thiserror::Error;

use crate::domain::{Follow, FollowError};
use crate::ports::{FollowStore, RepoError};

#[derive(Debug, Error)]
pub enum FollowServiceError {
    #[error(transparent)]
    Validation(#[from] FollowError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct FollowService {
    store: Arc<dyn FollowStore>,
}

impl FollowService {
    pub fn new(store: Arc<dyn FollowStore>) -> Self {
        Self { store }
    }

    pub async fn follow(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
        user_id: &str,
    ) -> Result<bool, FollowServiceError> {
        let f = Follow::new(project_id, entity_type, entity_id, user_id)?;
        Ok(self.store.follow(&f).await?)
    }

    pub async fn unfollow(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
        user_id: &str,
    ) -> Result<bool, FollowServiceError> {
        let f = Follow::new(project_id, entity_type, entity_id, user_id)?;
        Ok(self.store.unfollow(&f.project_id, &f.entity_type, &f.entity_id, &f.user_id).await?)
    }

    pub async fn followers(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Vec<String>, FollowServiceError> {
        // Placeholder non-empty user to reuse Follow::new's validation + entity_type normalization.
        let f = Follow::new(project_id, entity_type, entity_id, "-")?;
        Ok(self.store.followers(&f.project_id, &f.entity_type, &f.entity_id).await?)
    }

    pub async fn is_following(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
        user_id: &str,
    ) -> Result<bool, FollowServiceError> {
        let f = Follow::new(project_id, entity_type, entity_id, user_id)?;
        let people = self.store.followers(&f.project_id, &f.entity_type, &f.entity_id).await?;
        Ok(people.iter().any(|u| u == &f.user_id))
    }

    pub async fn following_ids(
        &self,
        project_id: &str,
        user_id: &str,
        entity_type: Option<&str>,
    ) -> Result<Vec<String>, FollowServiceError> {
        let project_id = project_id.trim();
        let user_id = user_id.trim();
        if project_id.is_empty() {
            return Err(FollowError::EmptyProject.into());
        }
        if user_id.is_empty() {
            return Err(FollowError::EmptyUser.into());
        }
        let norm = entity_type.map(|t| t.trim().to_ascii_lowercase());
        Ok(self.store.following_ids(project_id, user_id, norm.as_deref()).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryFollowStore;

    fn svc() -> FollowService {
        FollowService::new(Arc::new(InMemoryFollowStore::new()))
    }

    #[tokio::test]
    async fn follow_is_idempotent_and_listed() {
        let s = svc();
        assert!(s.follow("p1", "Bug", "b1", "alice").await.expect("ok"));
        assert!(!s.follow("p1", "bug", "b1", "alice").await.expect("ok"));
        assert!(s.is_following("p1", "BUG", "b1", "alice").await.expect("ok"));
        assert_eq!(s.followers("p1", "bug", "b1").await.expect("ok"), vec!["alice".to_string()]);
        assert_eq!(
            s.following_ids("p1", "alice", Some("bug")).await.expect("ok"),
            vec!["b1".to_string()]
        );
    }

    #[tokio::test]
    async fn unfollow_removes_and_is_idempotent() {
        let s = svc();
        s.follow("p1", "bug", "b1", "alice").await.expect("ok");
        assert!(s.unfollow("p1", "bug", "b1", "alice").await.expect("ok"));
        assert!(!s.unfollow("p1", "bug", "b1", "alice").await.expect("ok"));
        assert!(!s.is_following("p1", "bug", "b1", "alice").await.expect("ok"));
        assert!(s.followers("p1", "bug", "b1").await.expect("ok").is_empty());
    }

    #[tokio::test]
    async fn following_ids_filters_by_type() {
        let s = svc();
        s.follow("p1", "bug", "b1", "alice").await.expect("ok");
        s.follow("p1", "requirement", "r1", "alice").await.expect("ok");
        s.follow("p1", "bug", "b2", "bob").await.expect("ok");
        let mut bugs = s.following_ids("p1", "alice", Some("bug")).await.expect("ok");
        bugs.sort();
        assert_eq!(bugs, vec!["b1".to_string()]);
        let mut all = s.following_ids("p1", "alice", None).await.expect("ok");
        all.sort();
        assert_eq!(all, vec!["b1".to_string(), "r1".to_string()]);
    }

    #[tokio::test]
    async fn rejects_blank_input() {
        let s = svc();
        assert!(matches!(
            s.follow("p1", "bug", "", "alice").await,
            Err(FollowServiceError::Validation(FollowError::EmptyEntityId))
        ));
    }
}
