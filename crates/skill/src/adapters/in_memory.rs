use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{NewSkill, Skill};
use crate::ports::{RepoError, SkillRepository};

#[derive(Default)]
struct State {
    skills: Vec<Skill>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemorySkillRepository {
    state: Arc<Mutex<State>>,
}

impl InMemorySkillRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SkillRepository for InMemorySkillRepository {
    async fn find_active_by_name(
        &self,
        project_id: &str,
        name: &str,
    ) -> Result<Option<Skill>, RepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .skills
            .iter()
            .find(|s| s.occupies_name() && s.project_id == project_id && s.name == name)
            .cloned())
    }

    async fn insert(&self, new: &NewSkill) -> Result<Skill, RepoError> {
        let mut st = self.state.lock().expect("lock");
        st.seq += 1;
        let s = Skill::from_new(&format!("skill-{}", st.seq), new);
        st.skills.push(s.clone());
        Ok(s)
    }

    async fn get(&self, id: &str) -> Result<Option<Skill>, RepoError> {
        Ok(self.state.lock().expect("lock").skills.iter().find(|s| s.id == id).cloned())
    }

    async fn list_active(&self, project_id: &str) -> Result<Vec<Skill>, RepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .skills
            .iter()
            .filter(|s| s.occupies_name() && s.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn save(&self, skill: &Skill) -> Result<(), RepoError> {
        let mut st = self.state.lock().expect("lock");
        if let Some(slot) = st.skills.iter_mut().find(|s| s.id == skill.id) {
            *slot = skill.clone();
        }
        Ok(())
    }
}
