use crate::domain::test_assets::{AssetResult, ScenarioInput, ServiceInput, SourceSnapshot};
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait AssetSource: Send + Sync {
    async fn linked_changes(&self, _source: &str, _external: &str) -> AssetResult<Value> {
        Ok(serde_json::json!({"items":[],"available":false}))
    }
    async fn sources(&self) -> AssetResult<Value>;
    async fn snapshot(&self, source_id: &str) -> AssetResult<SourceSnapshot>;
}
#[async_trait]
pub trait AssetStore: Send + Sync {
    async fn set_case_api(
        &self,
        project: &str,
        case_id: &str,
        links: &[Value],
    ) -> AssetResult<Value>;
    async fn authorize(&self, project: &str, user: &str, admin: bool) -> AssetResult<()>;
    async fn overview(&self, project: &str) -> AssetResult<Value>;
    async fn save_service(
        &self,
        project: &str,
        id: Option<&str>,
        input: &ServiceInput,
    ) -> AssetResult<Value>;
    async fn delete_service(&self, project: &str, id: &str) -> AssetResult<()>;
    async fn assign_cases(
        &self,
        project: &str,
        service: Option<&str>,
        cases: &[String],
    ) -> AssetResult<()>;
    async fn save_mapping(&self, project: &str, input: &Value) -> AssetResult<()>;
    async fn preview_import(
        &self,
        project: &str,
        user: &str,
        snapshot: &SourceSnapshot,
    ) -> AssetResult<Value>;
    async fn import_item(
        &self,
        project: &str,
        id: &str,
        kind: &str,
        external_id: &str,
    ) -> AssetResult<Value>;
    async fn import_batch(&self, project: &str, id: &str) -> AssetResult<Value>;
    async fn apply_import(
        &self,
        project: &str,
        user: &str,
        id: &str,
        source_revision: &str,
        resolutions: &Value,
    ) -> AssetResult<Value>;
    async fn save_scenario(
        &self,
        project: &str,
        user: &str,
        id: Option<&str>,
        input: &ScenarioInput,
    ) -> AssetResult<Value>;
    async fn delete_scenario(&self, project: &str, id: &str) -> AssetResult<()>;
    async fn record_run(
        &self,
        project: &str,
        user: &str,
        id: &str,
        input: &Value,
    ) -> AssetResult<Value>;
    async fn scenario_runs(&self, project: &str, id: &str) -> AssetResult<Value>;
    async fn save_inventory(
        &self,
        project: &str,
        user: &str,
        service: &str,
        input: &Value,
    ) -> AssetResult<Value>;
}
