use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{FunctionalCase, NewFunctionalCase};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone)]
pub struct CoverageCase {
    pub criterion_index: i32,
    pub case_id: String,
    pub case_name: String,
    pub module: String,
    pub priority: String,
}

#[derive(Debug, Clone)]
pub struct CaseRequirement {
    pub requirement_id: String,
    pub requirement_title: String,
    pub criterion_index: i32,
}

#[async_trait]
pub trait CaseRepository: Send + Sync {
    async fn insert(&self, c: &NewFunctionalCase) -> Result<FunctionalCase, RepoError>;

    async fn update(&self, id: &str, c: &NewFunctionalCase) -> Result<Option<FunctionalCase>, RepoError>;

    async fn delete(&self, id: &str) -> Result<bool, RepoError>;

    async fn list_by_project(&self, project_id: &str) -> Result<Vec<FunctionalCase>, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<FunctionalCase>, RepoError>;

    async fn link_requirement_case(
        &self,
        requirement_id: &str,
        criterion_index: i32,
        functional_case_id: &str,
        project_id: &str,
    ) -> Result<(), RepoError>;

    async fn unlink_requirement_case(
        &self,
        requirement_id: &str,
        criterion_index: i32,
        functional_case_id: &str,
    ) -> Result<(), RepoError>;

    async fn cases_for_requirement(&self, requirement_id: &str) -> Result<Vec<CoverageCase>, RepoError>;

    async fn requirements_for_case(&self, functional_case_id: &str) -> Result<Vec<CaseRequirement>, RepoError>;
}
