//! 场景仓储端口。提供场景与步骤的读写。

use async_trait::async_trait;

use crate::domain::{
    ApiScenario, NewApiScenario, NewScenarioStep, ScenarioExecution, ScenarioReference,
    ScenarioStep,
};

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

    /// 更新场景基本信息:名称/状态/元信息(meta 整体替换)。返回更新后的场景(steps 已加载);
    /// 场景不存在返回 Ok(None)。
    async fn update_scenario(
        &self,
        id: &str,
        name: &str,
        status: &str,
        meta: &serde_json::Value,
    ) -> Result<Option<ApiScenario>, RepoError>;

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

    /// 删除场景内某步骤(按 scenario_id + step_id)。返回是否命中删除。
    async fn delete_step(&self, scenario_id: &str, step_id: &str) -> Result<bool, RepoError>;

    /// 记录一次场景执行。status 为状态字符串("PENDING"/"RUNNING"/"SUCCESS"/"ERROR"),
    /// 非法值由调用方在领域层保证;仓储原样落库。
    async fn record_execution(
        &self,
        scenario_id: &str,
        project_id: &str,
        status: &str,
        case_count: i32,
        report_id: Option<&str>,
    ) -> Result<ScenarioExecution, RepoError>;

    /// 统计某场景的执行记录总数。
    async fn count_executions(&self, scenario_id: &str) -> Result<u64, RepoError>;

    /// 分页列出某场景的执行记录,按 created_at 降序(最新在前)。
    async fn list_executions(
        &self,
        scenario_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<ScenarioExecution>, RepoError>;

    /// 引用关系反查:返回引用了给定任一用例(步骤 kind=CASE 且 ref_id ∈ case_ids)的场景(去重)。
    /// case_ids 为空时返回空。用于「接口定义 → 被哪些场景引用」。
    async fn list_scenarios_referencing_cases(
        &self,
        case_ids: &[String],
    ) -> Result<Vec<ScenarioReference>, RepoError>;
}
