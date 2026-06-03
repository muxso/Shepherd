//! 场景仓储端口。提供场景与步骤的读写。

use async_trait::async_trait;

use crate::domain::{ApiScenario, NewApiScenario, NewScenarioStep, ScenarioStep};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait ApiScenarioRepository: Send + Sync {
    /// 插入场景(返回的场景 steps 为空)。
    async fn insert_scenario(
        &self,
        s: &NewApiScenario,
    ) -> Result<ApiScenario, RepoError>;

    /// 按 id 取场景(steps 已加载,按 order 升序)。
    async fn get_scenario(&self, id: &str) -> Result<Option<ApiScenario>, RepoError>;

    /// 列出某项目的场景(排除已删除;steps 已加载)。
    async fn list_scenarios(
        &self,
        project_id: &str,
    ) -> Result<Vec<ApiScenario>, RepoError>;

    /// 为场景追加一个步骤。
    async fn add_step(
        &self,
        scenario_id: &str,
        step: &NewScenarioStep,
    ) -> Result<ScenarioStep, RepoError>;
}
