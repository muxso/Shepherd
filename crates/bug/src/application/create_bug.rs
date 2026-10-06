use std::collections::BTreeMap;
use std::sync::Arc;

use crate::domain::{
    normalize_custom_fields, normalize_handler, normalize_severity, Bug, BugError, NewBug,
};
use crate::ports::{BugRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateBugError {
    #[error(transparent)]
    Validation(#[from] BugError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateBugUseCase {
    repo: Arc<dyn BugRepository>,
}

impl CreateBugUseCase {
    pub fn new(repo: Arc<dyn BugRepository>) -> Self {
        Self { repo }
    }

    /// Exposes the repository so other use cases (e.g. custom fields) can share the same storage.
    pub fn repo(&self) -> Arc<dyn BugRepository> {
        self.repo.clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        project_id: &str,
        title: &str,
        initial_status: &str,
        created_by: Option<&str>,
        severity: Option<&str>,
        handler: Option<&str>,
        description: Option<&str>,
        custom_fields: &BTreeMap<String, String>,
    ) -> Result<Bug, CreateBugError> {
        let custom_fields = normalize_custom_fields(custom_fields)?;
        let severity = normalize_severity(severity)?;
        let new_bug = NewBug::new(project_id, title)?
            .with_created_by(created_by)
            .with_severity(severity)
            .with_handler(normalize_handler(handler))
            .with_description(description.map(|s| s.to_string()))
            .with_custom_fields(custom_fields);

        let flow = self.repo.status_flow(project_id).await?;
        if !flow.contains(initial_status) {
            return Err(CreateBugError::Validation(BugError::UnknownStatus(
                initial_status.to_string(),
            )));
        }

        Ok(self.repo.insert(&new_bug, initial_status).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryBugRepository;

    #[tokio::test]
    async fn creates_bug_with_valid_initial_status() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = CreateBugUseCase::new(Arc::new(repo));
        let bug = uc
            .execute("p1", "login crash", "NEW", Some("alice"), None, None, None, &BTreeMap::new())
            .await
            .expect("ok");
        assert_eq!(bug.status, "NEW");
        assert_eq!(bug.title, "login crash");
        assert_eq!(bug.created_by.as_deref(), Some("alice"));
        // Insert seeds the audit pair from the creator.
        assert_eq!(bug.updated_by.as_deref(), Some("alice"));
        assert!(bug.updated_at.is_some());
        assert!(bug.custom_fields.is_empty());
    }

    #[tokio::test]
    async fn creates_bug_with_severity_and_handler() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = CreateBugUseCase::new(Arc::new(repo));
        let bug = uc
            .execute(
                "p1",
                "login crash",
                "NEW",
                None,
                Some(" p0 "),
                Some(" bob "),
                None,
                &BTreeMap::new(),
            )
            .await
            .expect("ok");
        assert_eq!(bug.severity.as_deref(), Some("P0"));
        assert_eq!(bug.handler.as_deref(), Some("bob"));
    }

    #[tokio::test]
    async fn rejects_invalid_severity() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = CreateBugUseCase::new(Arc::new(repo));
        let err = uc
            .execute("p1", "x", "NEW", None, Some("SEV1"), None, None, &BTreeMap::new())
            .await
            .unwrap_err();
        assert_eq!(err, CreateBugError::Validation(BugError::InvalidSeverity("SEV1".into())));
    }

    #[tokio::test]
    async fn creates_bug_with_custom_fields_normalized() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = CreateBugUseCase::new(Arc::new(repo));
        let raw = BTreeMap::from([(" severity ".to_string(), "P0".to_string())]);
        let bug =
            uc.execute("p1", "login crash", "NEW", None, None, None, None, &raw).await.expect("ok");
        assert_eq!(bug.custom_fields, BTreeMap::from([("severity".to_string(), "P0".to_string())]));
        // Blank key is a validation error.
        let bad = BTreeMap::from([("  ".to_string(), "v".to_string())]);
        assert_eq!(
            uc.execute("p1", "crashed again", "NEW", None, None, None, None, &bad)
                .await
                .unwrap_err(),
            CreateBugError::Validation(BugError::EmptyCustomFieldKey)
        );
    }

    #[tokio::test]
    async fn rejects_unknown_initial_status() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = CreateBugUseCase::new(Arc::new(repo));
        let err = uc
            .execute("p1", "x", "GHOST", None, None, None, None, &BTreeMap::new())
            .await
            .unwrap_err();
        assert_eq!(err, CreateBugError::Validation(BugError::UnknownStatus("GHOST".into())));
    }

    #[tokio::test]
    async fn rejects_blank_title() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let uc = CreateBugUseCase::new(Arc::new(repo));
        let err = uc
            .execute("p1", "  ", "NEW", None, None, None, None, &BTreeMap::new())
            .await
            .unwrap_err();
        assert_eq!(err, CreateBugError::Validation(BugError::EmptyTitle));
    }
}
