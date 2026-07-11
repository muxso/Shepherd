use std::sync::Arc;

use crate::ports::{BugRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BugFollowerError {
    #[error("bug not found")]
    BugNotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct BugFollowersUseCase {
    repo: Arc<dyn BugRepository>,
}

impl BugFollowersUseCase {
    pub fn new(repo: Arc<dyn BugRepository>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, bug_id: &str) -> Result<Vec<String>, BugFollowerError> {
        self.ensure_bug(bug_id).await?;
        Ok(self.repo.list_followers(bug_id).await?)
    }

    pub async fn follow(
        &self,
        bug_id: &str,
        user_id: &str,
    ) -> Result<Vec<String>, BugFollowerError> {
        self.ensure_bug(bug_id).await?;
        self.repo.add_follower(bug_id, user_id).await?;
        Ok(self.repo.list_followers(bug_id).await?)
    }

    pub async fn unfollow(
        &self,
        bug_id: &str,
        user_id: &str,
    ) -> Result<Vec<String>, BugFollowerError> {
        self.ensure_bug(bug_id).await?;
        self.repo.remove_follower(bug_id, user_id).await?;
        Ok(self.repo.list_followers(bug_id).await?)
    }

    async fn ensure_bug(&self, bug_id: &str) -> Result<(), BugFollowerError> {
        self.repo.get(bug_id).await?.map(|_| ()).ok_or(BugFollowerError::BugNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryBugRepository;
    use crate::application::CreateBugUseCase;

    async fn seed_bug(repo: &InMemoryBugRepository) -> String {
        CreateBugUseCase::new(Arc::new(repo.clone()))
            .execute("p1", "boom", "NEW", None)
            .await
            .expect("seed")
            .id
    }

    #[tokio::test]
    async fn follow_is_idempotent_and_listed() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let id = seed_bug(&repo).await;
        let uc = BugFollowersUseCase::new(Arc::new(repo));

        uc.follow(&id, "u1").await.expect("follow");
        let followers = uc.follow(&id, "u1").await.expect("follow again");
        assert_eq!(followers, vec!["u1".to_string()]);

        let followers = uc.follow(&id, "u2").await.expect("follow u2");
        assert_eq!(followers, vec!["u1".to_string(), "u2".to_string()]);
    }

    #[tokio::test]
    async fn unfollow_removes_and_is_idempotent() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let id = seed_bug(&repo).await;
        let uc = BugFollowersUseCase::new(Arc::new(repo));

        uc.follow(&id, "u1").await.expect("follow");
        let followers = uc.unfollow(&id, "u1").await.expect("unfollow");
        assert!(followers.is_empty());
        assert!(uc.unfollow(&id, "ghost").await.expect("noop").is_empty());
    }

    #[tokio::test]
    async fn ops_on_missing_bug_are_not_found() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = BugFollowersUseCase::new(Arc::new(repo));
        assert_eq!(uc.list("ghost").await, Err(BugFollowerError::BugNotFound));
        assert_eq!(uc.follow("ghost", "u1").await, Err(BugFollowerError::BugNotFound));
        assert_eq!(uc.unfollow("ghost", "u1").await, Err(BugFollowerError::BugNotFound));
    }
}
