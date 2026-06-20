//! 远程执行历史记录(零 IO):某次「中央派给 agent 执行」的结果存档。

/// 一条远程执行记录(派发的目标 + agent 回传结果 + 时间)。
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
