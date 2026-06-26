use std::sync::Arc;

use thiserror::Error;

use crate::domain::{NewUser, User, UserError};
use crate::ports::{RepoError, UserRepository};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateUserError {
    #[error(transparent)]
    Validation(#[from] UserError),
    #[error("email already exists")]
    EmailAlreadyExists,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateUserUseCase {
    repo: Arc<dyn UserRepository>,
}

impl CreateUserUseCase {
    pub fn new(repo: Arc<dyn UserRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, name: &str, email: &str) -> Result<User, CreateUserError> {
        let new_user = NewUser::new(name, email)?;
        if self.repo.find_by_email(&new_user.email).await?.is_some() {
            return Err(CreateUserError::EmailAlreadyExists);
        }
        Ok(self.repo.insert(&new_user).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryUserRepository;

    #[tokio::test]
    async fn creates_user_when_email_is_new() {
        let uc = CreateUserUseCase::new(Arc::new(InMemoryUserRepository::new()));
        let user = uc.execute("Alice", "alice@example.com").await.expect("create");
        assert_eq!(user.name, "Alice");
        assert!(!user.id.is_empty());
    }

    #[tokio::test]
    async fn rejects_duplicate_email() {
        let uc = CreateUserUseCase::new(Arc::new(InMemoryUserRepository::new()));
        uc.execute("Alice", "alice@example.com").await.expect("first");
        let err = uc.execute("Alice2", "  Alice@Example.com ").await.unwrap_err();
        assert_eq!(err, CreateUserError::EmailAlreadyExists);
    }

    #[tokio::test]
    async fn propagates_validation_error() {
        let uc = CreateUserUseCase::new(Arc::new(InMemoryUserRepository::new()));
        let err = uc.execute("Bob", "nope").await.unwrap_err();
        assert_eq!(err, CreateUserError::Validation(UserError::InvalidEmail));
    }

    #[tokio::test]
    async fn persists_only_one_on_success() {
        let repo = InMemoryUserRepository::new();
        let uc = CreateUserUseCase::new(Arc::new(repo.clone()));
        uc.execute("Alice", "alice@example.com").await.expect("ok");
        assert_eq!(repo.count(), 1);
    }
}
