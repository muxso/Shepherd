use async_trait::async_trait;

use crate::ports::PortError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseExecutionRecord {
    pub report_id: String,
    pub case_id: String,
    pub outcome: String,
    pub failures: serde_json::Value,
    pub executed_at: String,
}

#[async_trait]
pub trait CaseExecutionQueryPort: Send + Sync {
    async fn count_by_case(&self, case_id: &str) -> Result<u64, PortError>;

    async fn list_by_case(
        &self,
        case_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<CaseExecutionRecord>, PortError>;
}
