use std::collections::BTreeMap;
use std::sync::Arc;

use crate::domain::{normalize_custom_fields, Bug, BugError};
use crate::ports::{BugRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BugCustomFieldsError {
    #[error("bug not found")]
    BugNotFound,
    #[error(transparent)]
    Validation(#[from] BugError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

/// Replaces a bug's custom field values wholesale (empty map clears all); field
/// definitions live in the project template.
#[derive(Clone)]
pub struct BugCustomFieldsUseCase {
    repo: Arc<dyn BugRepository>,
}

impl BugCustomFieldsUseCase {
    pub fn new(repo: Arc<dyn BugRepository>) -> Self {
        Self { repo }
    }

    pub async fn replace(
        &self,
        bug_id: &str,
        fields: &BTreeMap<String, String>,
    ) -> Result<Bug, BugCustomFieldsError> {
        let fields = normalize_custom_fields(fields)?;
        let mut bug = self.repo.get(bug_id).await?.ok_or(BugCustomFieldsError::BugNotFound)?;
        self.repo.set_custom_fields(bug_id, &fields).await?;
        bug.custom_fields = fields;
        Ok(bug)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryBugRepository;
    use crate::application::CreateBugUseCase;

    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    async fn seed_bug(repo: &InMemoryBugRepository) -> String {
        CreateBugUseCase::new(Arc::new(repo.clone()))
            .execute("p1", "boom", "NEW", None, &fields(&[("severity", "P1")]))
            .await
            .expect("seed")
            .id
    }

    #[tokio::test]
    async fn replace_overwrites_whole_map_and_persists() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let id = seed_bug(&repo).await;
        let uc = BugCustomFieldsUseCase::new(Arc::new(repo.clone()));

        let bug = uc
            .replace(&id, &fields(&[(" env ", "prod"), ("owner", "bob")]))
            .await
            .expect("replace");
        assert_eq!(bug.custom_fields, fields(&[("env", "prod"), ("owner", "bob")]));
        // Persisted in the repository (severity was replaced away).
        let uc2 = BugCustomFieldsUseCase::new(Arc::new(repo));
        let cleared = uc2.replace(&id, &BTreeMap::new()).await.expect("clear");
        assert!(cleared.custom_fields.is_empty());
    }

    #[tokio::test]
    async fn replace_rejects_invalid_fields_and_missing_bug() {
        let repo = InMemoryBugRepository::with_default_flow("p1");
        let id = seed_bug(&repo).await;
        let uc = BugCustomFieldsUseCase::new(Arc::new(repo));
        assert_eq!(
            uc.replace(&id, &fields(&[("  ", "v")])).await.unwrap_err(),
            BugCustomFieldsError::Validation(BugError::EmptyCustomFieldKey)
        );
        assert_eq!(
            uc.replace("ghost", &BTreeMap::new()).await.unwrap_err(),
            BugCustomFieldsError::BugNotFound
        );
    }
}
