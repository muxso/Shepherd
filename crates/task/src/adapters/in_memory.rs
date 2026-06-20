//! 内存版任务拆分仓储(test double 兼本地存储)。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{Decomposition, TaskStatus};
use crate::ports::{RepoError, TaskRepository};

#[derive(Default)]
struct State {
    decompositions: Vec<Decomposition>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryTaskRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryTaskRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TaskRepository for InMemoryTaskRepository {
    async fn create_decomposition(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Decomposition, RepoError> {
        let mut st = self.state.lock().expect("lock poisoned");
        st.seq += 1;
        let d = Decomposition::new(&format!("decomp-{}", st.seq), requirement_id, requirement_version);
        st.decompositions.push(d.clone());
        Ok(d)
    }

    async fn find_by_requirement_version(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Option<Decomposition>, RepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .decompositions
            .iter()
            .find(|d| d.requirement_id == requirement_id && d.requirement_version == requirement_version)
            .cloned())
    }

    async fn get(&self, id: &str) -> Result<Option<Decomposition>, RepoError> {
        Ok(self.state.lock().expect("lock").decompositions.iter().find(|d| d.id == id).cloned())
    }

    async fn save(&self, decomposition: &Decomposition) -> Result<(), RepoError> {
        let mut st = self.state.lock().expect("lock");
        if let Some(slot) = st.decompositions.iter_mut().find(|d| d.id == decomposition.id) {
            *slot = decomposition.clone();
        }
        Ok(())
    }

    async fn save_task_status(
        &self,
        decomposition_id: &str,
        task_id: &str,
        status: TaskStatus,
    ) -> Result<(), RepoError> {
        // 锁内只改目标任务这一行,等价于 pg 的行级更新 → 并发推进无丢更新。
        let mut st = self.state.lock().expect("lock");
        if let Some(d) = st.decompositions.iter_mut().find(|d| d.id == decomposition_id) {
            if let Some(t) = d.tasks.iter_mut().find(|t| t.id == task_id) {
                t.status = status;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NewTask;

    #[tokio::test]
    async fn create_get_save_roundtrip() {
        let repo = InMemoryTaskRepository::new();
        let mut d = repo.create_decomposition("req1", 1).await.expect("create");
        assert_eq!(d.id, "decomp-1");
        d.add_task(NewTask::new("A", "", &[], &[]).expect("v")).expect("add");
        repo.save(&d).await.expect("save");

        let got = repo.get(&d.id).await.expect("get").expect("some");
        assert_eq!(got.tasks.len(), 1);
        assert!(repo.find_by_requirement_version("req1", 1).await.expect("f").is_some());
        assert!(repo.find_by_requirement_version("req1", 2).await.expect("f").is_none());
    }
}
