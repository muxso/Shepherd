use std::sync::Arc;

use kernel::page::{Page, PageRequest};
use thiserror::Error;

use crate::domain::{Email, User, UserError};
use crate::ports::{RepoError, UserRepository};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UserCmdError {
    #[error(transparent)]
    Validation(#[from] UserError),
    #[error("email already exists")]
    EmailExists,
    #[error("user not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct UserService {
    repo: Arc<dyn UserRepository>,
}

impl UserService {
    pub fn new(repo: Arc<dyn UserRepository>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, page: PageRequest) -> Result<Page<User>, UserCmdError> {
        let total = self.repo.count_active().await?;
        let items = self.repo.list_active(page.offset(), page.page_size()).await?;
        Ok(Page::of(page, total, items))
    }

    pub async fn get(&self, id: &str) -> Result<User, UserCmdError> {
        self.repo.get(id).await?.filter(|u| !u.deleted).ok_or(UserCmdError::NotFound)
    }

    pub async fn update(
        &self,
        id: &str,
        name: &str,
        email: &str,
        enable: bool,
    ) -> Result<User, UserCmdError> {
        let mut user = self.get(id).await?;
        let new_email = Email::parse(email)?;
        if new_email != user.email {
            if let Some(existing) = self.repo.find_by_email(&new_email).await? {
                if existing.id != id {
                    return Err(UserCmdError::EmailExists);
                }
            }
            user.set_email(email)?;
        }
        user.rename(name)?;
        user.set_enabled(enable);
        self.repo.save(&user).await?;
        Ok(user)
    }

    pub async fn delete(&self, id: &str) -> Result<(), UserCmdError> {
        let mut user = self.get(id).await?;
        user.soft_delete();
        self.repo.save(&user).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryUserRepository;
    use crate::application::CreateUserUseCase;

    async fn seeded() -> (UserService, Arc<InMemoryUserRepository>, String) {
        let repo = Arc::new(InMemoryUserRepository::new());
        let id = CreateUserUseCase::new(repo.clone())
            .execute("Alice", "alice@example.com")
            .await
            .expect("seed")
            .id;
        (UserService::new(repo.clone()), repo, id)
    }

    #[tokio::test]
    async fn get_and_list() {
        let (svc, _r, id) = seeded().await;
        assert_eq!(svc.get(&id).await.expect("get").name, "Alice");
        let page = svc.list(PageRequest::new(1, 10).expect("v")).await.expect("list");
        assert_eq!(page.total, 1);
    }

    #[tokio::test]
    async fn update_name_email_enable() {
        let (svc, _r, id) = seeded().await;
        let u = svc.update(&id, "Alice2", "alice2@example.com", false).await.expect("update");
        assert_eq!(u.name, "Alice2");
        assert_eq!(u.email.as_str(), "alice2@example.com");
        assert!(!u.enable);
    }

    #[tokio::test]
    async fn update_to_taken_email_rejected() {
        let (svc, repo, id) = seeded().await;
        CreateUserUseCase::new(repo).execute("Bob", "bob@example.com").await.expect("b");
        assert_eq!(
            svc.update(&id, "Alice", "bob@example.com", true).await.unwrap_err(),
            UserCmdError::EmailExists
        );
    }

    #[tokio::test]
    async fn delete_then_not_found_and_email_freed() {
        let (svc, repo, id) = seeded().await;
        svc.delete(&id).await.expect("del");
        assert_eq!(svc.get(&id).await.unwrap_err(), UserCmdError::NotFound);
        assert!(CreateUserUseCase::new(repo).execute("A2", "alice@example.com").await.is_ok());
    }
}
