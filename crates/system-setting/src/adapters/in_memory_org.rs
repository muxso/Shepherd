use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{NewOrganization, Organization};
use crate::ports::{OrgRepoError, OrgRepository};

#[derive(Default)]
struct State {
    orgs: Vec<Organization>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryOrgRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryOrgRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl OrgRepository for InMemoryOrgRepository {
    async fn insert(&self, new_org: &NewOrganization) -> Result<Organization, OrgRepoError> {
        let mut st = self.state.lock().expect("lock");
        st.seq += 1;
        let org = Organization::active(&format!("org-{}", st.seq), &new_org.name);
        st.orgs.push(org.clone());
        Ok(org)
    }

    async fn find_active_by_name(&self, name: &str) -> Result<Option<Organization>, OrgRepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .orgs
            .iter()
            .find(|o| o.occupies_name() && o.name == name)
            .cloned())
    }

    async fn get(&self, id: &str) -> Result<Option<Organization>, OrgRepoError> {
        Ok(self.state.lock().expect("lock").orgs.iter().find(|o| o.id == id).cloned())
    }

    async fn count_active(&self) -> Result<u64, OrgRepoError> {
        Ok(self.state.lock().expect("lock").orgs.iter().filter(|o| o.occupies_name()).count() as u64)
    }

    async fn list_active(&self, offset: u64, limit: u32) -> Result<Vec<Organization>, OrgRepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .orgs
            .iter()
            .filter(|o| o.occupies_name())
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn save(&self, org: &Organization) -> Result<(), OrgRepoError> {
        let mut st = self.state.lock().expect("lock");
        if let Some(slot) = st.orgs.iter_mut().find(|o| o.id == org.id) {
            *slot = org.clone();
        }
        Ok(())
    }
}
