use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{PassRule, ReviewRecord, ReviewSetting, ReviewStatus};
use crate::ports::{
    NewReview, RepoError, ReviewCaseStatus, ReviewDetail, ReviewMetaUpdate, ReviewRepository,
    ReviewSummary,
};

/// Full review row for the in-memory backend (mirrors ms_case_review).
#[derive(Clone)]
struct StoredReview {
    review: NewReview,
    created_at: String,
}

#[derive(Default)]
struct State {
    settings: HashMap<String, ReviewSetting>,
    histories: HashMap<(String, String), Vec<ReviewRecord>>,
    case_status: HashMap<(String, String), ReviewStatus>,
    reviews: HashMap<String, StoredReview>,
    next_id: usize,
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

    async fn create_review(&self, req: &NewReview) -> Result<String, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        st.next_id += 1;
        let id = format!("rev{}", st.next_id);
        let rule = PassRule::parse(&req.pass_rule).unwrap_or(PassRule::Single);
        st.settings
            .insert(id.clone(), ReviewSetting { rule, reviewer_count: req.reviewer_count.max(1) });
        for c in &req.case_ids {
            st.case_status.insert((id.clone(), c.clone()), ReviewStatus::UnReviewed);
        }
        st.reviews.insert(
            id.clone(),
            StoredReview { review: req.clone(), created_at: "1970-01-01T00:00:00Z".to_string() },
        );
        Ok(id)
    }

    async fn update_review_meta(
        &self,
        review_id: &str,
        update: &ReviewMetaUpdate,
    ) -> Result<(), RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let stored = st.reviews.get_mut(review_id).ok_or(RepoError::NotFound)?;
        stored.review.pass_rule = update.pass_rule.clone();
        stored.review.reviewer_count = update.reviewer_count;
        stored.review.meta = update.meta.clone();
        let rule = PassRule::parse(&update.pass_rule).unwrap_or(PassRule::Single);
        st.settings.insert(
            review_id.to_string(),
            ReviewSetting { rule, reviewer_count: update.reviewer_count.max(1) },
        );
        Ok(())
    }

    async fn list_reviews(&self, project_id: &str) -> Result<Vec<ReviewSummary>, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut out: Vec<ReviewSummary> = st
            .reviews
            .iter()
            .filter(|(_, s)| s.review.project_id == project_id)
            .map(|(id, s)| {
                let total = s.review.case_ids.len();
                let passed = s
                    .review
                    .case_ids
                    .iter()
                    .filter(|c| {
                        st.case_status.get(&(id.clone(), (*c).clone())) == Some(&ReviewStatus::Pass)
                    })
                    .count();
                let mut reviewers: Vec<String> = st
                    .histories
                    .iter()
                    .filter(|((rid, _), _)| rid == id)
                    .flat_map(|(_, recs)| recs.iter().map(|r| r.reviewer_id.clone()))
                    .collect();
                reviewers.sort();
                reviewers.dedup();
                ReviewSummary {
                    id: id.clone(),
                    pass_rule: s.review.pass_rule.clone(),
                    reviewer_count: s.review.reviewer_count,
                    total,
                    passed,
                    created_at: s.created_at.clone(),
                    status: if total > 0 && passed >= total { "COMPLETED" } else { "IN_PROGRESS" }
                        .to_string(),
                    created_by: Some(s.review.created_by.clone()),
                    reviewers,
                    meta: s.review.meta.clone(),
                }
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    async fn get_review(&self, review_id: &str) -> Result<ReviewDetail, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let stored = st.reviews.get(review_id).ok_or(RepoError::NotFound)?;
        let cases = stored
            .review
            .case_ids
            .iter()
            .map(|c| ReviewCaseStatus {
                case_id: c.clone(),
                status: st
                    .case_status
                    .get(&(review_id.to_string(), c.clone()))
                    .map(|s| s.as_str().to_string())
                    .unwrap_or_else(|| "UN_REVIEWED".to_string()),
            })
            .collect();
        Ok(ReviewDetail {
            id: review_id.to_string(),
            pass_rule: stored.review.pass_rule.clone(),
            reviewer_count: stored.review.reviewer_count,
            cases,
        })
    }
}
