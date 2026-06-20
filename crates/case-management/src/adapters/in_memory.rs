//! 内存用例仓储(测试)。

use std::sync::Mutex;

use async_trait::async_trait;

use crate::domain::{FunctionalCase, NewFunctionalCase};
use crate::ports::{CaseRepository, RepoError};

#[derive(Default)]
pub struct InMemoryCaseRepository {
    cases: Mutex<Vec<FunctionalCase>>,
}

impl InMemoryCaseRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CaseRepository for InMemoryCaseRepository {
    async fn insert(&self, c: &NewFunctionalCase) -> Result<FunctionalCase, RepoError> {
        let mut g = self.cases.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        let view = FunctionalCase {
            id: format!("c{}", g.len() + 1),
            project_id: c.project_id.clone(),
            name: c.name.clone(),
            module: c.module.clone(),
            priority: c.priority.clone(),
            status: c.status.clone(),
            custom_fields: c.custom_fields.clone(),
        };
        g.push(view.clone());
        Ok(view)
    }

    async fn list_by_project(&self, project_id: &str) -> Result<Vec<FunctionalCase>, RepoError> {
        let g = self.cases.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        Ok(g.iter().filter(|c| c.project_id == project_id).cloned().collect())
    }

    async fn get(&self, id: &str) -> Result<Option<FunctionalCase>, RepoError> {
        let g = self.cases.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        Ok(g.iter().find(|c| c.id == id).cloned())
    }
}
