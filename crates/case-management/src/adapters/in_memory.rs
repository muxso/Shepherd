//! 内存用例仓储(测试)。

use std::sync::Mutex;

use async_trait::async_trait;

use crate::domain::{FunctionalCase, NewFunctionalCase};
use crate::ports::{CaseRepository, CaseRequirement, CoverageCase, RepoError};

#[derive(Default)]
pub struct InMemoryCaseRepository {
    cases: Mutex<Vec<FunctionalCase>>,
    /// (requirement_id, criterion_index, functional_case_id, project_id)
    links: Mutex<Vec<(String, i32, String, String)>>,
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
            steps: c.steps.clone(),
            created_by: c.created_by.clone(),
        };
        g.push(view.clone());
        Ok(view)
    }

    async fn update(&self, id: &str, c: &NewFunctionalCase) -> Result<Option<FunctionalCase>, RepoError> {
        let mut g = self.cases.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        let Some(existing) = g.iter_mut().find(|x| x.id == id) else { return Ok(None) };
        existing.name = c.name.clone();
        existing.module = c.module.clone();
        existing.priority = c.priority.clone();
        existing.status = c.status.clone();
        existing.custom_fields = c.custom_fields.clone();
        existing.steps = c.steps.clone();
        Ok(Some(existing.clone()))
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError> {
        let mut g = self.cases.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        let before = g.len();
        g.retain(|c| c.id != id);
        Ok(g.len() != before)
    }

    async fn list_by_project(&self, project_id: &str) -> Result<Vec<FunctionalCase>, RepoError> {
        let g = self.cases.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        Ok(g.iter().filter(|c| c.project_id == project_id).cloned().collect())
    }

    async fn get(&self, id: &str) -> Result<Option<FunctionalCase>, RepoError> {
        let g = self.cases.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        Ok(g.iter().find(|c| c.id == id).cloned())
    }

    async fn link_requirement_case(&self, requirement_id: &str, criterion_index: i32, functional_case_id: &str, project_id: &str) -> Result<(), RepoError> {
        let mut g = self.links.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        let key = (requirement_id.to_string(), criterion_index, functional_case_id.to_string());
        if !g.iter().any(|(r, i, c, _)| (r.as_str(), *i, c.as_str()) == (key.0.as_str(), key.1, key.2.as_str())) {
            g.push((key.0, key.1, key.2, project_id.to_string()));
        }
        Ok(())
    }

    async fn unlink_requirement_case(&self, requirement_id: &str, criterion_index: i32, functional_case_id: &str) -> Result<(), RepoError> {
        let mut g = self.links.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        g.retain(|(r, i, c, _)| !(r == requirement_id && *i == criterion_index && c == functional_case_id));
        Ok(())
    }

    async fn cases_for_requirement(&self, requirement_id: &str) -> Result<Vec<CoverageCase>, RepoError> {
        let links = self.links.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        let cases = self.cases.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        Ok(links
            .iter()
            .filter(|(r, ..)| r == requirement_id)
            .filter_map(|(_, idx, cid, _)| {
                let c = cases.iter().find(|c| &c.id == cid)?;
                Some(CoverageCase { criterion_index: *idx, case_id: c.id.clone(), case_name: c.name.clone(), module: c.module.clone(), priority: c.priority.clone() })
            })
            .collect())
    }

    async fn requirements_for_case(&self, functional_case_id: &str) -> Result<Vec<CaseRequirement>, RepoError> {
        let links = self.links.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        Ok(links
            .iter()
            .filter(|(_, _, c, _)| c == functional_case_id)
            .map(|(r, idx, _, _)| CaseRequirement { requirement_id: r.clone(), requirement_title: String::new(), criterion_index: *idx })
            .collect())
    }
}
