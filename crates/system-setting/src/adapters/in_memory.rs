use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{Email, NewUser, User};
use crate::ports::{RepoError, UserRepository};

#[derive(Default)]
struct State {
    users: Vec<User>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryUserRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryUserRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn count(&self) -> usize {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).users.len()
    }
}

#[async_trait]
impl UserRepository for InMemoryUserRepository {
    async fn find_by_email(&self, email: &Email) -> Result<Option<User>, RepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .users
            .iter()
            .find(|u| u.occupies_email() && u.email == *email)
            .cloned())
    }

    async fn insert(&self, new_user: &NewUser) -> Result<User, RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.seq += 1;
        let user = User {
            id: format!("user-{}", state.seq),
            name: new_user.name.clone(),
            email: new_user.email.clone(),
            enable: true,
            deleted: false,
        };
        state.users.push(user.clone());
        Ok(user)
    }

    async fn get(&self, id: &str) -> Result<Option<User>, RepoError> {
        Ok(self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).users.iter().find(|u| u.id == id).cloned())
    }

    async fn count_active(&self) -> Result<u64, RepoError> {
        Ok(self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).users.iter().filter(|u| u.occupies_email()).count()
            as u64)
    }

    async fn list_active(&self, offset: u64, limit: u32) -> Result<Vec<User>, RepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .users
            .iter()
            .filter(|u| u.occupies_email())
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn save(&self, user: &User) -> Result<(), RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(slot) = st.users.iter_mut().find(|u| u.id == user.id) {
            *slot = user.clone();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_then_find() {
        let repo = InMemoryUserRepository::new();
        let nu = NewUser::new("Alice", "alice@example.com").expect("valid");
        let inserted = repo.insert(&nu).await.expect("ok");
        assert_eq!(inserted.id, "user-1");
        assert_eq!(repo.find_by_email(&nu.email).await.expect("ok"), Some(inserted));
    }

    #[tokio::test]
    async fn find_missing_is_none() {
        let repo = InMemoryUserRepository::new();
        let email = Email::parse("nobody@example.com").expect("valid");
        assert_eq!(repo.find_by_email(&email).await.expect("ok"), None);
    }
}
