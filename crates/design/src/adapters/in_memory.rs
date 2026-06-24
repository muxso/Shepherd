//! 内存版提案仓储(测试 / 单机)。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;

use crate::domain::Proposal;
use crate::ports::{ProposalRepository, RepoError};

#[derive(Default)]
pub struct InMemoryProposalRepository {
    seq: AtomicU64,
    by_id: Mutex<HashMap<String, Proposal>>,
}

impl InMemoryProposalRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ProposalRepository for InMemoryProposalRepository {
    async fn create(&self, requirement_id: &str, title: &str) -> Result<Proposal, RepoError> {
        let id = format!("prop-{}", self.seq.fetch_add(1, Ordering::Relaxed) + 1);
        let p = Proposal::new(&id, requirement_id, title)
            .map_err(|e| RepoError::Backend(e.to_string()))?;
        self.by_id.lock().unwrap().insert(id, p.clone());
        Ok(p)
    }

    async fn get(&self, id: &str) -> Result<Option<Proposal>, RepoError> {
        Ok(self.by_id.lock().unwrap().get(id).cloned())
    }

    async fn save(&self, proposal: &Proposal) -> Result<(), RepoError> {
        self.by_id.lock().unwrap().insert(proposal.id.clone(), proposal.clone());
        Ok(())
    }

    async fn list_by_requirement(&self, requirement_id: &str) -> Result<Vec<Proposal>, RepoError> {
        let mut out: Vec<Proposal> = self
            .by_id
            .lock()
            .unwrap()
            .values()
            .filter(|p| p.requirement_id == requirement_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }
}
