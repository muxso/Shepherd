use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{NewSkill, Skill};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait SkillRepository: Send + Sync {
    async fn find_active_by_name(
        &self,
        project_id: &str,
        name: &str,
    ) -> Result<Option<Skill>, RepoError>;

    async fn insert(&self, new: &NewSkill) -> Result<Skill, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<Skill>, RepoError>;

    async fn list_active(&self, project_id: &str) -> Result<Vec<Skill>, RepoError>;

    async fn save(&self, skill: &Skill) -> Result<(), RepoError>;
}
