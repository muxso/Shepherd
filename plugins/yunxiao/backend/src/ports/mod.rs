use crate::domain::{
    CompleteRun, Error, ListQuery, MemberMetricsQuery, NewSource, Record, RequirementOutputQuery,
};
use async_trait::async_trait;
use serde_json::Value;
#[async_trait]
pub trait Store: Send + Sync {
    async fn sources(&self) -> Result<Value, Error>;
    async fn create_source(&self, source: NewSource) -> Result<Value, Error>;
    async fn start(&self, source: &str) -> Result<Value, Error>;
    async fn stage(&self, run: &str, records: Vec<Record>) -> Result<Value, Error>;
    async fn complete(&self, run: &str, body: CompleteRun) -> Result<Value, Error>;
    async fn fail(&self, run: &str, error: &str) -> Result<Value, Error>;
    async fn dashboard(&self, source: &str, days: i64) -> Result<Value, Error>;
    async fn requirement_output(&self, query: RequirementOutputQuery) -> Result<Value, Error>;
    async fn member_metrics(&self, source: &str, query: MemberMetricsQuery)
        -> Result<Value, Error>;
    async fn records(&self, source: &str, query: ListQuery) -> Result<Value, Error>;
}
