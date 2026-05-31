//! 用例:为一个需求版本开启完整性验证(每版本至多一个;验收标准为快照传入)。

use std::sync::Arc;

use thiserror::Error;

use crate::domain::{NewVerification, Verification, VerificationError};
use crate::ports::{RepoError, VerificationRepository};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateVerificationError {
    #[error(transparent)]
    Validation(#[from] VerificationError),
    #[error("verification already exists for this requirement version")]
    AlreadyExists,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateVerificationUseCase {
    repo: Arc<dyn VerificationRepository>,
}

impl CreateVerificationUseCase {
    pub fn new(repo: Arc<dyn VerificationRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        requirement_id: &str,
        requirement_version: u32,
        criteria: &[String],
    ) -> Result<Verification, CreateVerificationError> {
        let new = NewVerification::new(requirement_id, requirement_version, criteria)?;
        if self
            .repo
            .find_by_requirement_version(&new.requirement_id, new.requirement_version)
            .await?
            .is_some()
        {
            return Err(CreateVerificationError::AlreadyExists);
        }
        Ok(self.repo.create(&new).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryVerificationRepository;

    fn uc() -> CreateVerificationUseCase {
        CreateVerificationUseCase::new(Arc::new(InMemoryVerificationRepository::new()))
    }

    #[tokio::test]
    async fn creates_with_criteria_snapshot() {
        let v = uc().execute("req1", 1, &["c1".into(), "c2".into()]).await.expect("ok");
        assert_eq!(v.criteria.len(), 2);
        assert!(!v.is_complete()); // 尚无覆盖
    }

    #[tokio::test]
    async fn rejects_empty_requirement_and_criterion() {
        assert!(matches!(
            uc().execute("  ", 1, &[]).await.unwrap_err(),
            CreateVerificationError::Validation(VerificationError::EmptyRequirement)
        ));
    }

    #[tokio::test]
    async fn one_verification_per_requirement_version() {
        let uc = uc();
        uc.execute("req1", 1, &["c1".into()]).await.expect("first");
        assert_eq!(
            uc.execute("req1", 1, &["c1".into()]).await.unwrap_err(),
            CreateVerificationError::AlreadyExists
        );
        assert!(uc.execute("req1", 2, &["c1".into()]).await.is_ok());
    }
}
