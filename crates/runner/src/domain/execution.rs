#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRecord {
    pub id: String,
    pub agent_id: String,
    pub method: String,
    pub url: String,
    pub outcome: String,
    pub status: Option<u16>,
    pub elapsed_ms: Option<u64>,
    pub failures: Vec<String>,
    pub executed_at: String,
}
