use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{NewVerification, Verification};
use crate::ports::{RepoError, VerificationRepository};

#[derive(Default)]
struct State {
    verifications: Vec<Verification>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryVerificationRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryVerificationRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl VerificationRepository for InMemoryVerificationRepository {
    async fn create(&self, new: &NewVerification) -> Result<Verification, RepoError> {
        let mut st = self.state.lock().expect("lock poisoned");
        st.seq += 1;
        let v = Verification::from_new(&format!("verification-{}", st.seq), new);
        st.verifications.push(v.clone());
        Ok(v)
    }

    async fn find_by_requirement_version(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Option<Verification>, RepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .verifications
            .iter()
            .find(|v| v.requirement_id == requirement_id && v.requirement_version == requirement_version)
            .cloned())
    }

    async fn get(&self, id: &str) -> Result<Option<Verification>, RepoError> {
        Ok(self.state.lock().expect("lock").verifications.iter().find(|v| v.id == id).cloned())
    }

    async fn save(&self, verification: &Verification) -> Result<(), RepoError> {
        let mut st = self.state.lock().expect("lock");
        if let Some(slot) = st.verifications.iter_mut().find(|v| v.id == verification.id) {
            *slot = verification.clone();
        }
        Ok(())
    }
}
