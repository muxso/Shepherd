use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrpcCall {
    pub method: String,
    #[serde(default = "empty_message")]
    pub message: Value,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}
fn empty_message() -> Value {
    serde_json::json!({})
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "encoding", content = "value", rename_all = "snake_case")]
pub enum GrpcMetadataValue {
    Ascii(String),
    Base64(String),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcStatus {
    pub code: i32,
    pub message: String,
    pub details: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum GrpcReport {
    Connected {
        elapsed_ms: u64,
    },
    Called {
        method: String,
        status: GrpcStatus,
        response: Option<Value>,
        metadata: BTreeMap<String, Vec<GrpcMetadataValue>>,
        elapsed_ms: u64,
    },
}
impl GrpcReport {
    pub fn success(&self) -> bool {
        match self {
            Self::Connected { .. } => true,
            Self::Called { status, .. } => status.code == 0,
        }
    }
}
