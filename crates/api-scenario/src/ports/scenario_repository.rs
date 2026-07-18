use async_trait::async_trait;

use crate::domain::{
    ApiScenario, NewApiScenario, NewScenarioStep, ScenarioChange, ScenarioExecution,
    ScenarioReference, ScenarioStep,
};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait ApiScenarioRepository: Send + Sync {
    async fn insert_scenario(&self, s: &NewApiScenario) -> Result<ApiScenario, RepoError>;

    async fn get_scenario(&self, id: &str) -> Result<Option<ApiScenario>, RepoError>;

    async fn update_scenario(
        &self,
        id: &str,
        name: &str,
        status: &str,
        meta: &serde_json::Value,
    ) -> Result<Option<ApiScenario>, RepoError>;

    async fn list_scenarios(&self, project_id: &str) -> Result<Vec<ApiScenario>, RepoError>;

    async fn delete_scenario(&self, id: &str) -> Result<bool, RepoError>;

    /// Soft-deleted scenarios of a project (the recycle bin).
    async fn list_deleted(&self, project_id: &str) -> Result<Vec<ApiScenario>, RepoError>;

    /// Clears the deleted flag; false when the id is unknown or not deleted.
    async fn restore_scenario(&self, id: &str) -> Result<bool, RepoError>;

    /// Hard-deletes a soft-deleted scenario with its steps and executions.
    async fn purge_scenario(&self, id: &str) -> Result<bool, RepoError>;

    async fn add_step(
        &self,
        scenario_id: &str,
        step: &NewScenarioStep,
    ) -> Result<ScenarioStep, RepoError>;

    /// Replace a step's payload (kind, ref_mode, snapshot); `step.order` is ignored —
    /// ordering changes go through `reorder_steps`. Returns None if the step is missing.
    async fn update_step(
        &self,
        scenario_id: &str,
        step_id: &str,
        step: &NewScenarioStep,
    ) -> Result<Option<ScenarioStep>, RepoError>;

    async fn delete_step(&self, scenario_id: &str, step_id: &str) -> Result<bool, RepoError>;

    async fn reorder_steps(
        &self,
        scenario_id: &str,
        ordered_ids: &[String],
    ) -> Result<(), RepoError>;

    async fn record_change(
        &self,
        scenario_id: &str,
        action: &str,
        detail: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<(), RepoError>;

    async fn list_changes(&self, scenario_id: &str) -> Result<Vec<ScenarioChange>, RepoError>;

    async fn record_execution(
        &self,
        scenario_id: &str,
        project_id: &str,
        status: &str,
        case_count: i32,
        report_id: Option<&str>,
    ) -> Result<ScenarioExecution, RepoError>;

    async fn count_executions(&self, scenario_id: &str) -> Result<u64, RepoError>;

    async fn list_executions(
        &self,
        scenario_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<ScenarioExecution>, RepoError>;

    async fn list_scenarios_referencing_cases(
        &self,
        case_ids: &[String],
    ) -> Result<Vec<ScenarioReference>, RepoError>;
}
