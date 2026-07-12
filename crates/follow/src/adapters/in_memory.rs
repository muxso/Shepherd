use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::Follow;
use crate::ports::{FollowStore, RepoError};

#[derive(Clone, Default)]
pub struct InMemoryFollowStore {
    // Insertion order = follow-time order, which followers() relies on for a stable list.
    rows: Arc<Mutex<Vec<Follow>>>,
}

impl InMemoryFollowStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl FollowStore for InMemoryFollowStore {
    async fn follow(&self, f: &Follow) -> Result<bool, RepoError> {
        let mut rows = self.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if rows.iter().any(|r| r == f) {
            return Ok(false);
        }
        rows.push(f.clone());
        Ok(true)
    }

    async fn unfollow(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
        user_id: &str,
    ) -> Result<bool, RepoError> {
        let mut rows = self.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = rows.len();
        rows.retain(|r| {
            !(r.project_id == project_id
                && r.entity_type == entity_type
                && r.entity_id == entity_id
                && r.user_id == user_id)
        });
        Ok(rows.len() != before)
    }

    async fn followers(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Vec<String>, RepoError> {
        let rows = self.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(rows
            .iter()
            .filter(|r| {
                r.project_id == project_id
                    && r.entity_type == entity_type
                    && r.entity_id == entity_id
            })
            .map(|r| r.user_id.clone())
            .collect())
    }

    async fn following_ids(
        &self,
        project_id: &str,
        user_id: &str,
        entity_type: Option<&str>,
    ) -> Result<Vec<String>, RepoError> {
        let rows = self.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(rows
            .iter()
            .filter(|r| {
                r.project_id == project_id
                    && r.user_id == user_id
                    && match entity_type {
                        Some(t) => r.entity_type == t,
                        None => true,
                    }
            })
            .map(|r| r.entity_id.clone())
            .collect())
    }
}
