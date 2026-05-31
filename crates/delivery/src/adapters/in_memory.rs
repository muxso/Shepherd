//! 内存版交付仓储(test double 兼本地存储)。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{DeliveryAttempt, ExecutorKind};
use crate::ports::{DeliveryRepository, RepoError};

#[derive(Default)]
struct State {
    attempts: Vec<DeliveryAttempt>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryDeliveryRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryDeliveryRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl DeliveryRepository for InMemoryDeliveryRepository {
    async fn create(
        &self,
        decomposition_id: &str,
        task_id: &str,
        executor: ExecutorKind,
    ) -> Result<DeliveryAttempt, RepoError> {
        let mut st = self.state.lock().expect("lock poisoned");
        st.seq += 1;
        let a = DeliveryAttempt::dispatched(
            &format!("attempt-{}", st.seq),
            decomposition_id,
            task_id,
            executor,
        );
        st.attempts.push(a.clone());
        Ok(a)
    }

    async fn get(&self, id: &str) -> Result<Option<DeliveryAttempt>, RepoError> {
        Ok(self.state.lock().expect("lock").attempts.iter().find(|a| a.id == id).cloned())
    }

    async fn list_by_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<DeliveryAttempt>, RepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .attempts
            .iter()
            .filter(|a| a.decomposition_id == decomposition_id && a.task_id == task_id)
            .cloned()
            .collect())
    }

    async fn save(&self, attempt: &DeliveryAttempt) -> Result<(), RepoError> {
        let mut st = self.state.lock().expect("lock");
        if let Some(slot) = st.attempts.iter_mut().find(|a| a.id == attempt.id) {
            *slot = attempt.clone();
        }
        Ok(())
    }
}
