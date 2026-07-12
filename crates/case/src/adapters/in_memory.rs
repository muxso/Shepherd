use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{ReviewRecord, ReviewSetting, ReviewStatus};
use crate::ports::{RepoError, ReviewRepository};

#[derive(Default)]
struct State {
    settings: HashMap<String, ReviewSetting>,
    histories: HashMap<(String, String), Vec<ReviewRecord>>,
    case_status: HashMap<(String, String), ReviewStatus>,
}

#[derive(Clone, Default)]
pub struct InMemoryReviewRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryReviewRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_setting(&self, review_id: &str, setting: ReviewSetting) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .settings
            .insert(review_id.to_string(), setting);
    }

    pub fn case_status(&self, review_id: &str, case_id: &str) -> Option<ReviewStatus> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .case_status
            .get(&(review_id.to_string(), case_id.to_string()))
            .copied()
    }

    pub fn history_of_sync(&self, review_id: &str, case_id: &str) -> Vec<ReviewRecord> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .histories
            .get(&(review_id.to_string(), case_id.to_string()))
            .cloned()
            .unwrap_or_default()
    }
}

#[async_trait]
impl ReviewRepository for InMemoryReviewRepository {
    async fn review_setting(&self, review_id: &str) -> Result<ReviewSetting, RepoError> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .settings
            .get(review_id)
            .copied()
            .ok_or(RepoError::NotFound)
    }

    async fn history_of(
        &self,
        review_id: &str,
        case_id: &str,
    ) -> Result<Vec<ReviewRecord>, RepoError> {
        Ok(self.history_of_sync(review_id, case_id))
    }

    async fn append_history(
        &self,
        review_id: &str,
        case_id: &str,
        record: &ReviewRecord,
    ) -> Result<(), RepoError> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .histories
            .entry((review_id.to_string(), case_id.to_string()))
            .or_default()
            .push(record.clone());
        Ok(())
    }

    async fn set_case_status(
        &self,
        review_id: &str,
        case_id: &str,
        status: ReviewStatus,
    ) -> Result<(), RepoError> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .case_status
            .insert((review_id.to_string(), case_id.to_string()), status);
        Ok(())
    }
}
