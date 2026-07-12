use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{NewTemplate, Template};
use crate::ports::{RepoError, TemplateRepository};

#[derive(Default)]
struct State {
    rows: Vec<Template>,
    seq: u64,
    /// Logical clock: +1 on every write, so created/updated timestamps are monotonic and testable.
    clock: i64,
}

impl State {
    fn tick(&mut self) -> i64 {
        self.clock += 1;
        self.clock
    }
}

#[derive(Clone, Default)]
pub struct InMemoryTemplateRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryTemplateRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TemplateRepository for InMemoryTemplateRepository {
    async fn insert(&self, t: &NewTemplate) -> Result<Template, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        st.seq += 1;
        let now = st.tick();
        let row = Template {
            id: format!("template-{}", st.seq),
            project_id: t.project_id.clone(),
            kind: t.kind.clone(),
            name: t.name.clone(),
            config: t.config.clone(),
            created_by: t.created_by.clone(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        st.rows.push(row.clone());
        Ok(row)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Template>, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st.rows.iter().find(|r| r.id == id).cloned())
    }

    async fn find_by_name(
        &self,
        project_id: &str,
        kind: &str,
        name: &str,
    ) -> Result<Option<Template>, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st
            .rows
            .iter()
            .find(|r| r.project_id == project_id && r.kind == kind && r.name == name)
            .cloned())
    }

    async fn update(&self, t: &Template) -> Result<Option<Template>, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = st.tick();
        let Some(slot) = st.rows.iter_mut().find(|r| r.id == t.id) else {
            return Ok(None);
        };
        slot.name = t.name.clone();
        slot.config = t.config.clone();
        // Matches the pg side's `updated_at = now()`: every update stamps a new time.
        slot.updated_at_ms = now;
        Ok(Some(slot.clone()))
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = st.rows.len();
        st.rows.retain(|r| r.id != id);
        Ok(st.rows.len() != before)
    }

    async fn list(&self, project_id: &str, kind: Option<&str>) -> Result<Vec<Template>, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st
            .rows
            .iter()
            .filter(|r| r.project_id == project_id && kind.is_none_or(|k| r.kind == k))
            .cloned()
            .collect())
    }
}
