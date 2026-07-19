use std::sync::Arc;

use crate::domain::{Bug, BugRelation, RelationError, RelationKind};
use crate::ports::{BugRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BugRelationError {
    #[error("bug not found")]
    BugNotFound,
    #[error(transparent)]
    Domain(#[from] RelationError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct BugRelationsUseCase {
    repo: Arc<dyn BugRepository>,
}

impl BugRelationsUseCase {
    pub fn new(repo: Arc<dyn BugRepository>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, bug_id: &str) -> Result<Vec<BugRelation>, BugRelationError> {
        self.ensure_bug(bug_id).await?;
        Ok(self.repo.list_relations(bug_id).await?)
    }

    pub async fn link(
        &self,
        bug_id: &str,
        kind: &str,
        target_id: &str,
    ) -> Result<Vec<BugRelation>, BugRelationError> {
        self.ensure_bug(bug_id).await?;
        let rel = BugRelation::new(bug_id, kind, target_id)?;
        self.repo.add_relation(&rel).await?;
        Ok(self.repo.list_relations(bug_id).await?)
    }

    pub async fn unlink(
        &self,
        bug_id: &str,
        kind: &str,
        target_id: &str,
    ) -> Result<Vec<BugRelation>, BugRelationError> {
        self.ensure_bug(bug_id).await?;
        let rel = BugRelation::new(bug_id, kind, target_id)?;
        self.repo.remove_relation(&rel).await?;
        Ok(self.repo.list_relations(bug_id).await?)
    }

    /// Reverse lookup: bugs linked to a target entity (e.g. all bugs of a test plan).
    pub async fn bugs_for_target(
        &self,
        kind: &str,
        target_id: &str,
    ) -> Result<Vec<Bug>, BugRelationError> {
        let kind = RelationKind::parse(kind)?;
        let target_id = target_id.trim();
        if target_id.is_empty() {
            return Err(BugRelationError::Domain(RelationError::EmptyTarget));
        }
        Ok(self.repo.list_bugs_by_relation(kind, target_id).await?)
    }

    async fn ensure_bug(&self, bug_id: &str) -> Result<(), BugRelationError> {
        self.repo.get(bug_id).await?.map(|_| ()).ok_or(BugRelationError::BugNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryBugRepository;
    use crate::application::CreateBugUseCase;
    use crate::domain::RelationKind;

    async fn seed_bug(repo: &InMemoryBugRepository) -> String {
        CreateBugUseCase::new(Arc::new(repo.clone()))
            .execute("p1", "boom", "NEW", None, None, None, &std::collections::BTreeMap::new())
            .await
            .expect("seed")
            .id
    }

    #[tokio::test]
    async fn link_is_idempotent_and_listed() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let id = seed_bug(&repo).await;
        let uc = BugRelationsUseCase::new(Arc::new(repo));

        uc.link(&id, "REQUIREMENT", "r1").await.expect("link");
        let rels = uc.link(&id, "requirement", "r1").await.expect("link again");
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].kind, RelationKind::Requirement);
        assert_eq!(rels[0].target_id, "r1");

        let rels = uc.link(&id, "SCENARIO", "s1").await.expect("link scenario");
        assert_eq!(rels.len(), 2);
    }

    #[tokio::test]
    async fn unlink_removes_and_is_idempotent() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let id = seed_bug(&repo).await;
        let uc = BugRelationsUseCase::new(Arc::new(repo));

        uc.link(&id, "FUNCTIONAL_CASE", "c1").await.expect("link");
        let rels = uc.unlink(&id, "FUNCTIONAL_CASE", "c1").await.expect("unlink");
        assert!(rels.is_empty());
        assert!(uc.unlink(&id, "FUNCTIONAL_CASE", "ghost").await.expect("noop").is_empty());
    }

    #[tokio::test]
    async fn plan_link_and_reverse_lookup() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let a = seed_bug(&repo).await;
        let b = seed_bug(&repo).await;
        let uc = BugRelationsUseCase::new(Arc::new(repo));

        uc.link(&a, "PLAN", "plan-1").await.expect("link a");
        uc.link(&b, "plan", "plan-1").await.expect("link b");
        uc.link(&b, "PLAN", "plan-2").await.expect("link b2");

        // Newest first; only bugs linked to the queried plan.
        let bugs = uc.bugs_for_target("PLAN", "plan-1").await.expect("reverse");
        assert_eq!(
            bugs.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(),
            vec![b.as_str(), a.as_str()]
        );
        let bugs = uc.bugs_for_target("PLAN", "plan-2").await.expect("reverse");
        assert_eq!(bugs.len(), 1);
        assert_eq!(bugs[0].id, b);
        assert!(uc.bugs_for_target("PLAN", "ghost").await.expect("empty").is_empty());

        // Unlink drops the bug from the reverse view.
        uc.unlink(&b, "PLAN", "plan-1").await.expect("unlink");
        let bugs = uc.bugs_for_target("PLAN", "plan-1").await.expect("reverse");
        assert_eq!(bugs.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec![a.as_str()]);
    }

    #[tokio::test]
    async fn bugs_for_target_rejects_bad_input() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = BugRelationsUseCase::new(Arc::new(repo));
        assert!(matches!(
            uc.bugs_for_target("king", "plan-1").await,
            Err(BugRelationError::Domain(RelationError::UnknownKind(_)))
        ));
        assert!(matches!(
            uc.bugs_for_target("PLAN", "  ").await,
            Err(BugRelationError::Domain(RelationError::EmptyTarget))
        ));
    }

    #[tokio::test]
    async fn invalid_kind_or_target_rejected() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let id = seed_bug(&repo).await;
        let uc = BugRelationsUseCase::new(Arc::new(repo));
        assert!(matches!(
            uc.link(&id, "king", "r1").await,
            Err(BugRelationError::Domain(RelationError::UnknownKind(_)))
        ));
        assert!(matches!(
            uc.link(&id, "REQUIREMENT", "  ").await,
            Err(BugRelationError::Domain(RelationError::EmptyTarget))
        ));
    }

    #[tokio::test]
    async fn ops_on_missing_bug_are_not_found() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = BugRelationsUseCase::new(Arc::new(repo));
        assert_eq!(uc.list("ghost").await, Err(BugRelationError::BugNotFound));
        assert_eq!(uc.link("ghost", "REQUIREMENT", "r1").await, Err(BugRelationError::BugNotFound));
        assert_eq!(
            uc.unlink("ghost", "REQUIREMENT", "r1").await,
            Err(BugRelationError::BugNotFound)
        );
    }
}
