use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{NewRequirement, Requirement};
use crate::ports::{RepoError, RequirementRepository};

#[derive(Default)]
struct State {
    requirements: Vec<Requirement>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryRequirementRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryRequirementRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn soft_delete(&self, id: &str) {
        let mut st = self.state.lock().expect("lock poisoned");
        if let Some(r) = st.requirements.iter_mut().find(|r| r.id == id) {
            r.soft_delete();
        }
    }
}

#[async_trait]
impl RequirementRepository for InMemoryRequirementRepository {
    async fn find_active_by_title(
        &self,
        project_id: &str,
        title: &str,
    ) -> Result<Option<Requirement>, RepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock poisoned")
            .requirements
            .iter()
            .find(|r| r.occupies_title() && r.project_id == project_id && r.title == title)
            .cloned())
    }

    async fn insert(&self, new: &NewRequirement) -> Result<Requirement, RepoError> {
        let mut st = self.state.lock().expect("lock poisoned");
        st.seq += 1;
        let req = Requirement::create(&format!("requirement-{}", st.seq), new);
        st.requirements.push(req.clone());
        Ok(req)
    }

    async fn get(&self, id: &str) -> Result<Option<Requirement>, RepoError> {
        Ok(self.state.lock().expect("lock").requirements.iter().find(|r| r.id == id).cloned())
    }

    async fn count_active(&self, project_id: &str) -> Result<u64, RepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .requirements
            .iter()
            .filter(|r| r.occupies_title() && r.project_id == project_id)
            .count() as u64)
    }

    async fn list_active(
        &self,
        project_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<Requirement>, RepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .requirements
            .iter()
            .filter(|r| r.occupies_title() && r.project_id == project_id)
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn save(&self, requirement: &Requirement) -> Result<(), RepoError> {
        let mut st = self.state.lock().expect("lock");
        if let Some(slot) = st.requirements.iter_mut().find(|r| r.id == requirement.id) {
            *slot = requirement.clone();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_get_save_roundtrip() {
        let repo = InMemoryRequirementRepository::new();
        let nu = NewRequirement::new("p1", "登录", "d", &["c1".to_string()]).expect("v");
        let mut r = repo.insert(&nu).await.expect("insert");
        assert_eq!(r.id, "requirement-1");

        r.revise("v2", vec![]).expect("revise");
        repo.save(&r).await.expect("save");
        let got = repo.get(&r.id).await.expect("get").expect("some");
        assert_eq!(got.latest_version(), 2);
    }

    #[tokio::test]
    async fn find_active_ignores_soft_deleted() {
        let repo = InMemoryRequirementRepository::new();
        let nu = NewRequirement::new("p1", "登录", "d", &[]).expect("v");
        let r = repo.insert(&nu).await.expect("insert");
        assert!(repo.find_active_by_title("p1", "登录").await.expect("q").is_some());
        repo.soft_delete(&r.id);
        assert!(repo.find_active_by_title("p1", "登录").await.expect("q").is_none());
    }
}
