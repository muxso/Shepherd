use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentError {
    #[error("agent name must not be empty")]
    EmptyName,
    #[error("agent base url must not be empty")]
    EmptyBaseUrl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerAgent {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
    pub protocols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRunnerAgent {
    pub name: String,
    pub base_url: String,
    pub token: Option<String>,
    pub enabled: bool,
}

impl NewRunnerAgent {
    pub fn new(
        name: &str,
        base_url: &str,
        token: Option<String>,
        enabled: bool,
    ) -> Result<Self, AgentError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AgentError::EmptyName);
        }
        let base_url = base_url.trim().trim_end_matches('/');
        if base_url.is_empty() {
            return Err(AgentError::EmptyBaseUrl);
        }
        let token = token.filter(|t| !t.trim().is_empty());
        Ok(Self { name: name.to_string(), base_url: base_url.to_string(), token, enabled })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchTarget {
    pub base_url: String,
    pub token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTarget {
    pub id: String,
    pub name: String,
    pub target: DispatchTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteResult {
    pub outcome: String,
    pub status: Option<u16>,
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub failures: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_trims() {
        let a = NewRunnerAgent::new(
            " test environment ",
            "http://10.0.0.5:9100/",
            Some("t".into()),
            true,
        )
        .expect("ok");
        assert_eq!(a.name, "test environment");
        assert_eq!(a.base_url, "http://10.0.0.5:9100");
        assert_eq!(a.token.as_deref(), Some("t"));
    }

    #[test]
    fn blank_token_becomes_none() {
        let a = NewRunnerAgent::new("e", "http://x", Some("  ".into()), true).expect("ok");
        assert_eq!(a.token, None);
    }

    #[test]
    fn rejects_empty_name_and_url() {
        assert_eq!(
            NewRunnerAgent::new(" ", "http://x", None, true).unwrap_err(),
            AgentError::EmptyName
        );
        assert_eq!(
            NewRunnerAgent::new("e", "  ", None, true).unwrap_err(),
            AgentError::EmptyBaseUrl
        );
    }
}
