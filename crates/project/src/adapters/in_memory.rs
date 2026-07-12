use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{NewProject, Project};
use crate::ports::{ProjectRepository, RepoError};

#[derive(Default)]
struct State {
    projects: Vec<Project>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryProjectRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryProjectRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn soft_delete(&self, id: &str) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(p) = state.projects.iter_mut().find(|p| p.id == id) {
            p.soft_delete();
        }
    }
}

#[async_trait]
impl ProjectRepository for InMemoryProjectRepository {
    async fn find_active_by_name(
        &self,
        organization_id: &str,
        name: &str,
    ) -> Result<Option<Project>, RepoError> {
        let found = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .projects
            .iter()
            .find(|p| p.occupies_name() && p.organization_id == organization_id && p.name == name)
            .cloned();
        Ok(found)
    }

    async fn insert(&self, new_project: &NewProject) -> Result<Project, RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.seq += 1;
        let project = Project::active(
            &format!("project-{}", state.seq),
            &new_project.organization_id,
            &new_project.name,
        );
        state.projects.push(project.clone());
        Ok(project)
    }

    async fn count_active(&self, organization_id: &str) -> Result<u64, RepoError> {
        let n = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .projects
            .iter()
            .filter(|p| p.occupies_name() && p.organization_id == organization_id)
            .count();
        Ok(n as u64)
    }

    async fn list_active(
        &self,
        organization_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<Project>, RepoError> {
        let items = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .projects
            .iter()
            .filter(|p| p.occupies_name() && p.organization_id == organization_id)
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_assigns_sequential_ids() {
        let repo = InMemoryProjectRepository::new();
        let a = repo.insert(&NewProject::new("org1", "A").expect("v")).await.expect("ok");
        let b = repo.insert(&NewProject::new("org1", "B").expect("v")).await.expect("ok");
        assert_eq!(a.id, "project-1");
        assert_eq!(b.id, "project-2");
    }

    #[tokio::test]
    async fn find_active_ignores_soft_deleted() {
        let repo = InMemoryProjectRepository::new();
        let p = repo.insert(&NewProject::new("org1", "Demo").expect("v")).await.expect("ok");
        assert!(repo.find_active_by_name("org1", "Demo").await.expect("ok").is_some());

        repo.soft_delete(&p.id);
        assert!(repo.find_active_by_name("org1", "Demo").await.expect("ok").is_none());
    }
}
