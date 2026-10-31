//! SSH request/report types contain no transport dependencies or secret diagnostics.
use serde::{Deserialize, Serialize};
#[derive(Clone)]
pub struct SshTarget {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub host_key_sha256: String,
}
pub enum SshAuthentication<'a> {
    Password(&'a str),
    PrivateKey { key: &'a str, passphrase: Option<&'a str> },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SshCommand {
    pub command: String,
}
pub enum SshAction<'a> {
    Test,
    Execute(&'a SshCommand),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "encoding", content = "value", rename_all = "snake_case")]
pub enum SshOutput {
    Utf8(String),
    Hex(String),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum SshTermination {
    Exit { code: u32 },
    Signal { name: String, core_dumped: bool, message: String },
    Missing,
}
impl SshTermination {
    pub fn success(&self) -> bool {
        matches!(self, Self::Exit { code: 0 })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum SshReport {
    Connected {
        host_key_sha256: String,
        elapsed_ms: u64,
    },
    Executed {
        host_key_sha256: String,
        stdout: SshOutput,
        stderr: SshOutput,
        termination: SshTermination,
        elapsed_ms: u64,
    },
}
impl SshReport {
    pub fn success(&self) -> bool {
        match self {
            Self::Connected { .. } => true,
            Self::Executed { termination, .. } => termination.success(),
        }
    }
}
