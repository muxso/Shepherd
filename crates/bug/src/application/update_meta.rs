use std::sync::Arc;

use crate::domain::{normalize_handler, normalize_severity, Bug, BugError};
use crate::ports::{BugRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UpdateBugMetaError {
    #[error("bug not found")]
    BugNotFound,
    #[error(transparent)]
    Validation(#[from] BugError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

/// Updates a bug's meta fields (title/severity/handler). Severity/handler are
/// full replacements (absent clears); a missing title keeps the current one.
/// Every call stamps updated_by/updated_at with the operator.
#[derive(Clone)]
pub struct UpdateBugMetaUseCase {
    repo: Arc<dyn BugRepository>,
}

impl UpdateBugMetaUseCase {
    pub fn new(repo: Arc<dyn BugRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        bug_id: &str,
        title: Option<&str>,
        severity: Option<&str>,
        handler: Option<&str>,
        operator: Option<&str>,
    ) -> Result<Bug, UpdateBugMetaError> {
        let severity = normalize_severity(severity)?;
        let handler = normalize_handler(handler);
        let current = self.repo.get(bug_id).await?.ok_or(UpdateBugMetaError::BugNotFound)?;
        let title = match title.map(str::trim) {
            None | Some("") => current.title.clone(),
            Some(t) => t.to_string(),
        };
        self.repo
            .update_meta(bug_id, &title, severity.as_deref(), handler.as_deref(), operator)
            .await?
            .ok_or(UpdateBugMetaError::BugNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryBugRepository;
    use crate::application::CreateBugUseCase;

    async fn seed_bug(repo: &InMemoryBugRepository) -> Bug {
        CreateBugUseCase::new(Arc::new(repo.clone()))
            .execute(
                "p1",
                "boom",
                "NEW",
                Some("alice"),
                Some("P2"),
                Some("bob"),
                &std::collections::BTreeMap::new(),
            )
            .await
            .expect("seed")
    }

    #[tokio::test]
    async fn updates_fields_and_stamps_operator() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let seeded = seed_bug(&repo).await;
        let uc = UpdateBugMetaUseCase::new(Arc::new(repo));

        let bug = uc
            .execute(&seeded.id, Some("boom v2"), Some(" p0 "), Some(" carol "), Some("dave"))
            .await
            .expect("ok");
        assert_eq!(bug.title, "boom v2");
        assert_eq!(bug.severity.as_deref(), Some("P0"));
        assert_eq!(bug.handler.as_deref(), Some("carol"));
        assert_eq!(bug.updated_by.as_deref(), Some("dave"));
        assert_ne!(bug.updated_at, seeded.updated_at);
        // Creation audit stays intact.
        assert_eq!(bug.created_by.as_deref(), Some("alice"));
        assert_eq!(bug.status, "NEW");
    }

    #[tokio::test]
    async fn absent_severity_handler_clear_and_absent_title_keeps() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let seeded = seed_bug(&repo).await;
        let uc = UpdateBugMetaUseCase::new(Arc::new(repo));

        let bug = uc.execute(&seeded.id, None, None, None, Some("dave")).await.expect("ok");
        assert_eq!(bug.title, "boom");
        assert_eq!(bug.severity, None);
        assert_eq!(bug.handler, None);
    }

    #[tokio::test]
    async fn rejects_invalid_severity_and_missing_bug() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let seeded = seed_bug(&repo).await;
        let uc = UpdateBugMetaUseCase::new(Arc::new(repo));

        assert_eq!(
            uc.execute(&seeded.id, None, Some("HIGH"), None, None).await.unwrap_err(),
            UpdateBugMetaError::Validation(BugError::InvalidSeverity("HIGH".into()))
        );
        assert_eq!(
            uc.execute("ghost", None, None, None, None).await.unwrap_err(),
            UpdateBugMetaError::BugNotFound
        );
    }
}
